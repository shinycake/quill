use crate::ids::{ChatId, FileId, MessageId, RequestId, UserId};
use crate::text::{TextEntity, TextEntityKind, utf16_to_utf8_offset};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
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

#[derive(Debug, Clone, PartialEq)]
pub enum EnvelopePayload {
    UpdateAuthorizationState(AuthorizationState),
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
        reply_markup: Option<InlineKeyboard>,
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
    UpdateChatReadInbox {
        chat_id: ChatId,
        last_read_inbox_message_id: MessageId,
        unread_count: i32,
    },
    UpdateChatReadOutbox {
        chat_id: ChatId,
        last_read_outbox_message_id: MessageId,
    },
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
    },
    /// `updateChatDraftMessage`. Positions are the new chat-list orders.
    UpdateChatDraftMessage {
        chat_id: ChatId,
        draft: Option<ChatDraft>,
        positions: Vec<ChatPositionUpdate>,
    },
    /// Parity slice 4: `updateChatPermissions` (schema 1.8.67, line 10500).
    /// Only `can_send_basic_messages` is kept — the topic-composer gate.
    UpdateChatPermissions {
        chat_id: ChatId,
        can_send_basic_messages: bool,
    },
    /// `updateUser` — Phase 6 keeps the full parsed user (contacts list,
    /// user info panel) in `Session::users`; the bot bit still drives the
    /// private-chat draft gate.
    UpdateUser {
        user_id: UserId,
        user: ParsedUser,
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
    /// Parity slice: `updateChatPhoto` (schema 1.8.67, line 10488) — the
    /// chat's photo changed. `photo` is the new `chatPhotoInfo.small`
    /// file (`None` when the photo was removed).
    UpdateChatPhoto {
        chat_id: ChatId,
        photo: Option<ParsedFile>,
    },
    /// `updateChatAction` — peer activity (`chatActionTyping` / `chatActionCancel`).
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
    Ok,
    Error(TdError),
    Messages(Vec<ParsedMessage>),
    Message(ParsedMessage),
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
    /// `foundChatMessages` — `searchChatMessages`.
    FoundChatMessages {
        total_count: i32,
        messages: Vec<ParsedMessage>,
        next_from_message_id: MessageId,
    },
    /// `updateSupergroup` — `supergroup.is_forum` is how Quill learns a
    /// supergroup is a forum (`chatTypeSupergroup` has no forum flag).
    /// Parity slice: the first active username (`supergroup.usernames`,
    /// schema 1.8.67 lines 2746/2372) feeds the channel/supergroup header.
    UpdateSupergroup {
        supergroup_id: i64,
        is_forum: bool,
        username: String,
        /// Phase A1: own `chatMemberStatus*` (`supergroup.status`, schema
        /// 1.8.67 line 2746 — "Current user status in the supergroup or
        /// channel"). Drives slow-mode bypass (admins/creators are exempt).
        status: ChannelMemberStatus,
        /// Phase A1: `rights.can_restrict_members` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, lines 2500/1092);
        /// `None` for any other status or a missing rights block.
        /// `setChatSlowModeDelay` requires this right (line 13551).
        can_restrict_members: Option<bool>,
    },
    /// `supergroup` — `getSupergroup` response. Phase A1: also keeps own
    /// `status` (`supergroup.status`, schema 1.8.67 line 2746) for the
    /// slow-mode bypass check.
    Supergroup {
        supergroup_id: i64,
        is_forum: bool,
        username: String,
        status: ChannelMemberStatus,
        /// Phase A1: `rights.can_restrict_members` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, lines 2500/1092);
        /// `None` for any other status or a missing rights block.
        can_restrict_members: Option<bool>,
    },
    /// `forumTopics` — `getForumTopics` response. Only the first page is
    /// fetched; `next_offset_*` are dropped (see Phase 5.1 DECISIONS).
    ForumTopics {
        total_count: i32,
        topics: Vec<ForumTopic>,
    },
    UpdateFile(ParsedFile),
    File(ParsedFile),
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
        stickers: Vec<StickerItem>,
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
    /// `updateSavedAnimations` — file ids of saved GIFs, newest first.
    UpdateSavedAnimations {
        animation_ids: Vec<i32>,
    },
    /// `notificationSounds` — `getSavedNotificationSounds` response.
    NotificationSounds {
        sounds: Vec<NotificationSound>,
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
        bot_info: Option<BotInfo>,
        bio: String,
        /// Preferred size's `photo:file` from `chatPhoto.sizes`
        /// (schema 1.8.67, line 1030); `None` when the user has no photo.
        photo: Option<ParsedFile>,
    },
    /// `updateUserFullInfo` — full info changed (schema 1.8.67, line 10744);
    /// the user id is explicit here.
    UpdateUserFullInfo {
        user_id: UserId,
        bot_info: Option<BotInfo>,
        bio: String,
        photo: Option<ParsedFile>,
    },
    /// `supergroupFullInfo` — `getSupergroupFullInfo` response (schema
    /// 1.8.67, line 11513). The response carries no supergroup id; it is
    /// resolved from the pending request in `Session::apply`. Kept:
    /// `description`, `member_count`, `linked_chat_id` (schema 1.8.67,
    /// line 2792; the discussion-group chat id for the channel header's
    /// "Discuss" affordance), plus the slow-mode fields and boost counts
    /// (Phase A1: `slow_mode_delay` / `slow_mode_delay_expires_in`, schema
    /// 1.8.67 lines 2758–2759; `my_boost_count` / `unrestrict_boost_count`,
    /// lines 2779–2780) that drive composer slow-mode enforcement. Dropped:
    /// admin/restricted/banned counts, invite link, sticker sets, gift
    /// fields, paid-message and statistics flags, location.
    SupergroupFullInfo {
        description: String,
        member_count: i32,
        linked_chat_id: i64,
        slow_mode_delay: i32,
        slow_mode_delay_expires_in: f64,
        my_boost_count: i32,
        unrestrict_boost_count: i32,
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
    /// (it never went live). Unreachable without `sendStory` (absent from
    /// 1.8.67), parsed for schema completeness.
    UpdateStoryPostFailed {
        story: ParsedStory,
        error: TdError,
    },
    /// Phase 9.2: `availableReactions` — the `getStoryAvailableReactions`
    /// response (TDLib 1.8.67, `schema/td_api.tl:13802`). The reducer keeps
    /// it in `Session::story_available_reactions` for the viewer picker.
    StoryAvailableReactions {
        reactions: Vec<StoryAvailableReactionView>,
    },
    Unknown(UnknownKind),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownKind {
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TdError {
    pub code: i32,
    /// Error *class* only. The TDLib `message` field is not stored; it can
    /// contain phone numbers or other secrets.
    pub class: ErrorClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    NotFound,
    Unauthorized,
    Flood,
    Invalid,
    Other,
}

impl TdError {
    pub fn from_code(code: i32) -> Self {
        let class = match code {
            404 => ErrorClass::NotFound,
            401 => ErrorClass::Unauthorized,
            429 => ErrorClass::Flood,
            400 => ErrorClass::Invalid,
            _ => ErrorClass::Other,
        };
        Self { code, class }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationState {
    WaitTdlibParameters,
    WaitPhoneNumber,
    WaitPremiumPurchase,
    WaitEmailAddress,
    WaitEmailCode,
    WaitCode { code_length: Option<i32> },
    WaitOtherDeviceConfirmation,
    WaitRegistration,
    WaitPassword { has_recovery_email: bool },
    Ready,
    LoggingOut,
    Closing,
    Closed,
    Unknown(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    WaitingForNetwork,
    ConnectingToProxy,
    Connecting,
    Updating,
    Ready,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatKind {
    Private {
        user_id: UserId,
    },
    BasicGroup {
        basic_group_id: i64,
    },
    Supergroup {
        supergroup_id: i64,
        is_channel: bool,
    },
    Secret {
        secret_chat_id: i32,
        user_id: UserId,
    },
    Unknown,
}

impl ChatKind {
    /// Phase B1: secret chats are now supported — they render, open,
    /// and send through the same chat pipeline as cloud chats (TDLib
    /// handles the E2E crypto internally). The old name said "cloud";
    /// kept short since it gates general chat support now.
    pub fn is_supported_chat(&self) -> bool {
        match self {
            ChatKind::Private { .. } | ChatKind::BasicGroup { .. } => true,
            // Phase 2.2: broadcast channels are ungated (sponsored-content
            // handling landed in 2.1).
            ChatKind::Supergroup { .. } => true,
            // Phase B1: secret chats ungated.
            ChatKind::Secret { .. } => true,
            ChatKind::Unknown => false,
        }
    }

    pub fn gate_reason(&self) -> Option<&'static str> {
        match self {
            ChatKind::Unknown => Some("This conversation type is not supported yet."),
            _ => None,
        }
    }

    /// `chatTypeSupergroup` with `is_channel: true` (TDLib 1.8.67).
    pub fn is_channel(&self) -> bool {
        matches!(
            self,
            ChatKind::Supergroup {
                is_channel: true,
                ..
            }
        )
    }
}

/// Phase B1: `SecretChatState` (TDLib 1.8.67, `schema/td_api.tl:2795`):
/// `secretChatStatePending` (:2798, "waiting for the other user to get
/// online"), `secretChatStateReady` (:2801), `secretChatStateClosed`
/// (:2804).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretChatState {
    Pending,
    Ready,
    Closed,
    Unknown(String),
}

impl SecretChatState {
    pub fn from_type_name(type_name: &str) -> Self {
        match type_name {
            "secretChatStatePending" => SecretChatState::Pending,
            "secretChatStateReady" => SecretChatState::Ready,
            "secretChatStateClosed" => SecretChatState::Closed,
            other => SecretChatState::Unknown(other.to_string()),
        }
    }
}

/// Phase B1: `secretChat` subset (TDLib 1.8.67, `schema/td_api.tl:2816`):
/// `secretChat id:int32 user_id:int53 state:SecretChatState
/// is_outbound:Bool key_hash:bytes layer:int32 = SecretChat;`
/// `key_hash` (36 little-endian bytes) is kept raw for the B2 key
/// verification UI; the layer is kept for future capability gating.
///
/// Security: key material stays in memory only — never logged, never
/// written to disk. `Debug` is hand-written and prints only the hash
/// *length*, so formatting an envelope can never leak key bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct ParsedSecretChat {
    pub id: i32,
    pub user_id: i64,
    pub state: SecretChatState,
    pub is_outbound: bool,
    pub key_hash: Vec<u8>,
    pub layer: i32,
}

impl std::fmt::Debug for ParsedSecretChat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedSecretChat")
            .field("id", &self.id)
            .field("user_id", &self.user_id)
            .field("state", &self.state)
            .field("is_outbound", &self.is_outbound)
            .field("key_hash_len", &self.key_hash.len())
            .field("layer", &self.layer)
            .finish()
    }
}

fn parse_secret_chat(value: Option<&Value>) -> Option<ParsedSecretChat> {
    let value = value?;
    Some(ParsedSecretChat {
        id: value.get("id").and_then(Value::as_i64).unwrap_or(0) as i32,
        user_id: int53(value.get("user_id")).ok()?,
        state: SecretChatState::from_type_name(
            value
                .get("state")
                .and_then(|s| s.get("@type"))
                .and_then(Value::as_str)
                .unwrap_or(""),
        ),
        is_outbound: value
            .get("is_outbound")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        key_hash: value
            .get("key_hash")
            .and_then(Value::as_str)
            .and_then(|s| STANDARD.decode(s).ok())
            .unwrap_or_default(),
        layer: value.get("layer").and_then(Value::as_i64).unwrap_or(0) as i32,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatPositionUpdate {
    pub chat_id: ChatId,
    pub list: ChatList,
    pub order: i64,
    pub is_pinned: bool,
}

/// Phase C1: `CallDiscardReason` (TDLib 1.8.67,
/// `schema/td_api.tl:6981`): `callDiscardReasonEmpty` (:6984),
/// `callDiscardReasonMissed` (:6987), `callDiscardReasonDeclined`
/// (:6990), `callDiscardReasonDisconnected` (:6993),
/// `callDiscardReasonHungUp` (:6996),
/// `callDiscardReasonUpgradeToGroupCall` (:6999, carries an invite
/// link — group calls are C3, the link is kept but unused).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallDiscardReason {
    Empty,
    Missed,
    Declined,
    Disconnected,
    HungUp,
    UpgradeToGroupCall { invite_link: String },
    Unknown(String),
}

impl CallDiscardReason {
    pub fn from_value(value: Option<&Value>) -> Self {
        let type_name = value
            .and_then(|v| v.get("@type"))
            .and_then(Value::as_str)
            .unwrap_or("");
        match type_name {
            "callDiscardReasonEmpty" => CallDiscardReason::Empty,
            "callDiscardReasonMissed" => CallDiscardReason::Missed,
            "callDiscardReasonDeclined" => CallDiscardReason::Declined,
            "callDiscardReasonDisconnected" => CallDiscardReason::Disconnected,
            "callDiscardReasonHungUp" => CallDiscardReason::HungUp,
            "callDiscardReasonUpgradeToGroupCall" => CallDiscardReason::UpgradeToGroupCall {
                invite_link: value
                    .and_then(|v| v.get("invite_link"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            },
            other => CallDiscardReason::Unknown(other.to_string()),
        }
    }

    /// Human-readable ended-call line shown on the call-end screen.
    /// `is_outgoing` disambiguates "Declined" (they declined ours vs.
    /// we declined theirs).
    pub fn summary(&self, is_outgoing: bool) -> String {
        match self {
            CallDiscardReason::Empty => "Call ended".to_string(),
            CallDiscardReason::Missed => {
                if is_outgoing {
                    "Call not answered".to_string()
                } else {
                    "Missed call".to_string()
                }
            }
            CallDiscardReason::Declined => {
                if is_outgoing {
                    "Declined".to_string()
                } else {
                    "You declined the call".to_string()
                }
            }
            CallDiscardReason::Disconnected => "Call disconnected".to_string(),
            CallDiscardReason::HungUp => "Call ended".to_string(),
            CallDiscardReason::UpgradeToGroupCall { .. } => "Upgraded to a group call".to_string(),
            CallDiscardReason::Unknown(_) => "Call ended".to_string(),
        }
    }
}

/// Phase C1: `CallState` (TDLib 1.8.67, `schema/td_api.tl:7051`):
/// `callStatePending` (:7058, `is_created` / `is_received`),
/// `callStateExchangingKeys` (:7063), `callStateReady` (:7066),
/// `callStateHangingUp` (:7077), `callStateDiscarded` (:7080 —
/// `reason` / `need_rating` / `need_debug_information` / `need_log`),
/// `callStateError` (:7086). The `Ready` state's protocol / servers /
/// encryption key are media-transport material — Quill has no
/// transport yet (C2), so only the state tag is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallState {
    Pending {
        is_created: bool,
        is_received: bool,
    },
    ExchangingKeys,
    Ready,
    HangingUp,
    Discarded {
        reason: CallDiscardReason,
        need_rating: bool,
        need_debug_information: bool,
        need_log: bool,
    },
    Error {
        code: i32,
        message: String,
    },
    Unknown(String),
}

impl CallState {
    pub fn from_value(value: Option<&Value>) -> Self {
        let value = match value {
            Some(v) => v,
            None => return CallState::Unknown(String::new()),
        };
        let type_name = value.get("@type").and_then(Value::as_str).unwrap_or("");
        match type_name {
            "callStatePending" => CallState::Pending {
                is_created: value
                    .get("is_created")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                is_received: value
                    .get("is_received")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
            "callStateExchangingKeys" => CallState::ExchangingKeys,
            "callStateReady" => CallState::Ready,
            "callStateHangingUp" => CallState::HangingUp,
            "callStateDiscarded" => CallState::Discarded {
                reason: CallDiscardReason::from_value(value.get("reason")),
                need_rating: value
                    .get("need_rating")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                need_debug_information: value
                    .get("need_debug_information")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                need_log: value
                    .get("need_log")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
            "callStateError" => {
                let error = value.get("error");
                CallState::Error {
                    code: error
                        .and_then(|e| e.get("code"))
                        .and_then(Value::as_i64)
                        .unwrap_or(0) as i32,
                    message: error
                        .and_then(|e| e.get("message"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                }
            }
            other => CallState::Unknown(other.to_string()),
        }
    }

    /// Terminal states end the tracked call. `Unknown` is deliberately
    /// *not* terminal — a future state the schema doesn't know yet
    /// must not silently drop a live call; the UI labels it honestly.
    pub fn is_terminal(&self) -> bool {
        matches!(self, CallState::Discarded { .. } | CallState::Error { .. })
    }
}

/// Phase C1: `call` subset (TDLib 1.8.67, `schema/td_api.tl:7287`):
/// `call id:int32 unique_id:int64 user_id:int53 is_outgoing:Bool
/// is_video:Bool state:CallState = Call;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCall {
    pub id: i32,
    pub unique_id: i64,
    pub user_id: i64,
    pub is_outgoing: bool,
    pub is_video: bool,
    pub state: CallState,
}

fn parse_call(value: Option<&Value>) -> Option<ParsedCall> {
    let value = value?;
    Some(ParsedCall {
        id: value.get("id").and_then(Value::as_i64).unwrap_or(0) as i32,
        unique_id: int53_or_zero(value.get("unique_id")),
        user_id: int53(value.get("user_id")).ok()?,
        is_outgoing: value
            .get("is_outgoing")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_video: value
            .get("is_video")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        state: CallState::from_value(value.get("state")),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatList {
    Main,
    Archive,
    Folder(i32),
    Unknown,
}

/// `chatFolderInfo` (TDLib 1.8.67, `schema/td_api.tl:3485`). Only the fields
/// Quill needs for folder tabs: identifier, display name (plain text — the
/// schema allows only CustomEmoji entities in folder names, which are
/// dropped), icon name, and color id. `is_shareable` / `has_my_invite_links`
/// are dropped (folder create/edit/share is out of scope).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatFolderInfo {
    pub id: i32,
    pub name: String,
    pub icon_name: String,
    pub color_id: i32,
}

/// Parity slice: the full editable `chatFolder` spec (TDLib 1.8.67,
/// `schema/td_api.tl:3476`) behind `createChatFolder` / `editChatFolder`
/// (`:13358` / `:13361`) and returned by `getChatFolder` (`:13355`).
/// Quill always sends `icon: null` (default icon) and `color_id: -1`
/// (disabled) — custom-emoji icon rendering is out of scope — and
/// `is_shareable: false` (invite links out of scope). The remaining fields
/// are the create/edit dialog's model.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChatFolderSpec {
    pub name: String,
    pub pinned_chat_ids: Vec<i64>,
    pub included_chat_ids: Vec<i64>,
    pub excluded_chat_ids: Vec<i64>,
    pub exclude_muted: bool,
    pub exclude_read: bool,
    pub exclude_archived: bool,
    pub include_contacts: bool,
    pub include_non_contacts: bool,
    pub include_bots: bool,
    pub include_groups: bool,
    pub include_channels: bool,
}

/// tdesktop default mute submenu (`SessionSettings::mutePeriods` when unset /
/// `DefaultTimePickerValues`): 1 hour, 8 hours, 2 days. Seconds, matching
/// `chatNotificationSettings.mute_for`.
pub const MUTE_FOR_1_HOUR: i32 = 3600;
pub const MUTE_FOR_8_HOURS: i32 = 8 * 3600;
pub const MUTE_FOR_2_DAYS: i32 = 2 * 86400;
/// tdesktop `MuteMenu::kMuteForeverValue` (`numeric_limits<int>::max()`).
/// TDLib: mute_for longer than 366 days is muted forever.
pub const MUTE_FOREVER: i32 = i32::MAX;
pub const MUTE_FOREVER_AFTER_SECONDS: i32 = 366 * 86400;

/// `messageSenderUser` / `messageSenderChat` (TDLib 1.8.67).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageSender {
    User { user_id: i64 },
    Chat { chat_id: i64 },
}

/// Own `chatMemberStatus*` for a broadcast channel (TDLib 1.8.67).
/// Drives the composer gate (admins post in 2.3) and the join/leave affordance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelMemberStatus {
    Creator,
    Administrator,
    Member,
    Restricted,
    Left,
    Banned,
    Unknown,
}

impl ChannelMemberStatus {
    pub fn is_admin(self) -> bool {
        matches!(
            self,
            ChannelMemberStatus::Creator | ChannelMemberStatus::Administrator
        )
    }

    pub fn is_joined(self) -> bool {
        matches!(
            self,
            ChannelMemberStatus::Creator
                | ChannelMemberStatus::Administrator
                | ChannelMemberStatus::Member
        )
    }
}

/// Typed `chatMember` (TDLib 1.8.67). Only `member_id` and `status` are kept;
/// `tag` / `inviter_user_id` / `joined_chat_date` stay out of this slice.
/// `admin_can_post_messages` carries `rights.can_post_messages` from
/// `chatMemberStatusAdministrator` (schema 1.8.67:
/// `chatAdministratorRights ... can_post_messages:Bool ...`), driving the
/// channel-admin composer gate; `None` for every other status or when the
/// rights block is absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedChatMember {
    pub member_id: MessageSender,
    pub status: ChannelMemberStatus,
    pub admin_can_post_messages: Option<bool>,
}

/// `botCommand` (TDLib 1.8.67, `schema/td_api.tl:826`):
/// `botCommand command:string description:string is_ephemeral:Bool =
/// BotCommand`. `is_ephemeral` is not kept — the panel only lists commands;
/// tapping one inserts plain text into the composer (Phase 3.3 owns the
/// command menu).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotCommand {
    pub command: String,
    pub description: String,
}

/// `botInfo` subset (TDLib 1.8.67, `schema/td_api.tl:2430`): only what the
/// bot panel renders — `short_description`, `description`, and
/// `commands:vector<botCommand>` (a bare vector of `botCommand`, not the
/// `botCommands` wrapper). Photo, menu button, rights, and links are
/// intentionally not kept.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BotInfo {
    pub short_description: String,
    pub description: String,
    pub commands: Vec<BotCommand>,
}

/// Phase 6: `userStatus*` (TDLib 1.8.67, `schema/td_api.tl:6407`).
/// Unknown constructors fall back to `Empty` — a hostile status can never
/// crash the parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UserStatusKind {
    #[default]
    Empty,
    Online,
    Offline {
        was_online: i32,
    },
    Recently,
    LastWeek,
    LastMonth,
}

impl UserStatusKind {
    /// Cheap status line for the contacts list / user info panel.
    pub fn display(&self) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.display_at(now)
    }

    fn display_at(&self, now_secs: u64) -> String {
        match *self {
            UserStatusKind::Empty => String::new(),
            UserStatusKind::Online => "online".to_string(),
            UserStatusKind::Recently => "last seen recently".to_string(),
            UserStatusKind::LastWeek => "last seen within a week".to_string(),
            UserStatusKind::LastMonth => "last seen within a month".to_string(),
            UserStatusKind::Offline { was_online } => {
                let was = was_online.max(0) as u64;
                if was == 0 || was > now_secs {
                    return "last seen a long time ago".to_string();
                }
                let ago = now_secs - was;
                if ago < 60 {
                    "last seen just now".to_string()
                } else if ago < 3600 {
                    format!("last seen {}m ago", ago / 60)
                } else if ago < 86400 {
                    format!("last seen {}h ago", ago / 3600)
                } else if ago < 7 * 86400 {
                    format!("last seen {}d ago", ago / 86400)
                } else {
                    "last seen a long time ago".to_string()
                }
            }
        }
    }

    pub fn is_online(&self) -> bool {
        matches!(self, UserStatusKind::Online)
    }
}

/// Phase 6: `user` subset (TDLib 1.8.67, `schema/td_api.tl:2403`) kept for
/// the contacts list and the user info panel. Dropped (documented, not
/// forgotten): accent/background color ids, emoji status, verification
/// status, premium/support flags, restriction info, active story state,
/// new-chat restrictions, paid-message star count, access flags, chat
/// language, attachment-menu flag.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedUser {
    pub id: i64,
    pub first_name: String,
    pub last_name: String,
    /// First entry of `usernames.active_usernames` (schema 1.8.67, line
    /// 2372 — this schema version has no singular `username` field).
    pub username: String,
    pub phone_number: String,
    pub is_contact: bool,
    pub is_bot: bool,
    pub status: UserStatusKind,
    /// `profile_photo.small.id` (`profilePhoto`, schema 1.8.67 line 754);
    /// 0 = no photo.
    pub photo_small_file_id: i32,
}

impl ParsedUser {
    pub fn display_name(&self) -> String {
        let name = format!("{} {}", self.first_name, self.last_name);
        let name = name.trim();
        if name.is_empty() {
            format!("User {}", self.id)
        } else {
            name.to_string()
        }
    }

    /// Two-letter avatar fallback ("Ada Lovelace" → "AL").
    pub fn initials(&self) -> String {
        let mut out = String::new();
        for part in [&self.first_name, &self.last_name] {
            if let Some(ch) = part.chars().next() {
                out.push(ch);
                if out.chars().count() == 2 {
                    break;
                }
            }
        }
        if out.is_empty() {
            out.push('?');
        }
        out
    }
}

/// `buttonStyle*` (TDLib 1.8.67, `schema/td_api.tl:3696`). Unknown styles
/// fall back to `Default` — the keyboard must never fail to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InlineKeyboardButtonStyle {
    #[default]
    Default,
    Primary,
    Danger,
    Success,
    Link,
}

/// `targetChat*` for `inlineKeyboardButtonTypeSwitchInline`
/// (TDLib 1.8.67, `schema/td_api.tl:7476`). Only `Current` is actionable in
/// this slice; the rest insert into the current chat's composer too (chat
/// picker is out of scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineKeyboardTargetChat {
    Current,
    Chosen,
    InternalLink,
    Unknown,
}

/// `inlineKeyboardButtonType*` (TDLib 1.8.67, `schema/td_api.tl:3774`).
/// Types needing a TDLib round-trip or platform flow we do not have yet
/// (`LoginUrl` → `getLoginUrlInfo`, `WebApp` → `openWebApp`,
/// `CallbackWithPassword` → password prompt, `CallbackGame`, `Buy`,
/// `User`) are parsed and stored but render as disabled buttons. Anything
/// unrecognized becomes `Unknown` and also renders disabled — never a crash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineKeyboardButtonType {
    Url {
        url: String,
    },
    LoginUrl {
        url: String,
    },
    WebApp {
        url: String,
    },
    Callback {
        data: Vec<u8>,
    },
    CallbackWithPassword {
        data: Vec<u8>,
    },
    CallbackGame,
    SwitchInline {
        query: String,
        target: InlineKeyboardTargetChat,
    },
    Buy,
    User {
        user_id: i64,
    },
    CopyText {
        text: String,
    },
    Disabled,
    Unknown {
        type_name: String,
    },
}

/// `inlineKeyboardButton` (TDLib 1.8.67, `schema/td_api.tl:3828`).
/// `icon_custom_emoji_id` is parsed (schema presence) but not rendered yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineKeyboardButton {
    pub text: String,
    pub style: InlineKeyboardButtonStyle,
    pub kind: InlineKeyboardButtonType,
}

/// `replyMarkupInlineKeyboard` (TDLib 1.8.67, `schema/td_api.tl:3855`):
/// `rows` is a vector of rows (`vector<vector<inlineKeyboardButton>>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineKeyboard {
    pub rows: Vec<Vec<InlineKeyboardButton>>,
    pub force_reply: bool,
}

/// `callbackQueryAnswer` (TDLib 1.8.67, `schema/td_api.tl:7747`): the bot's
/// answer to a callback query sent via `getCallbackQueryAnswer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallbackQueryAnswer {
    pub text: String,
    pub show_alert: bool,
    pub url: String,
}

/// Typed `ChatJoinResult` — `joinChat` response (TDLib 1.8.67: no
/// invite-link variant exists in this schema). The other variants surface as
/// a fixed note (no TDLib text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatJoinResult {
    Success { chat_id: ChatId },
    RequestSent,
    GuardBotApprovalRequired,
    Declined,
}

/// `draftMessage` text this slice restores. Voice/video/rich drafts are ignored.
/// `reply_to` is same-chat `inputMessageReplyToMessage` only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatDraft {
    pub text: String,
    pub reply_to_message_id: Option<MessageId>,
}

/// `ChatAction` values this slice acts on. Other constructors stay `Other`
/// so a replacement action clears typing without inventing labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatAction {
    /// `chatActionTyping`
    Typing,
    /// `chatActionCancel`, or a null action (schema: null cancels).
    Cancel,
    Other,
}

/// `chatNotificationSettings` (TDLib 1.8.67). Other fields are copied through
/// so a mute change does not reset sound / preview exceptions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatNotificationSettings {
    pub use_default_mute_for: bool,
    pub mute_for: i32,
    pub use_default_sound: bool,
    pub sound_id: i64,
    pub use_default_show_preview: bool,
    pub show_preview: bool,
    pub use_default_mute_stories: bool,
    pub mute_stories: bool,
    pub use_default_story_sound: bool,
    pub story_sound_id: i64,
    pub use_default_show_story_poster: bool,
    pub show_story_poster: bool,
    pub use_default_disable_pinned_message_notifications: bool,
    pub disable_pinned_message_notifications: bool,
    pub use_default_disable_mention_notifications: bool,
    pub disable_mention_notifications: bool,
}

impl Default for ChatNotificationSettings {
    fn default() -> Self {
        Self {
            use_default_mute_for: true,
            mute_for: 0,
            use_default_sound: true,
            sound_id: 0,
            use_default_show_preview: true,
            show_preview: false,
            use_default_mute_stories: true,
            mute_stories: false,
            use_default_story_sound: true,
            story_sound_id: 0,
            use_default_show_story_poster: true,
            show_story_poster: false,
            use_default_disable_pinned_message_notifications: true,
            disable_pinned_message_notifications: false,
            use_default_disable_mention_notifications: true,
            disable_mention_notifications: false,
        }
    }
}

impl ChatNotificationSettings {
    /// Exception mute. `use_default_mute_for` stays true until the user sets one
    /// (Unigram clones settings and clears the default flag).
    pub fn with_mute_for(mut self, mute_for: i32) -> Self {
        self.use_default_mute_for = false;
        self.mute_for = mute_for;
        self
    }

    /// Effective chat mute. Scope defaults are not applied here.
    pub fn is_muted(&self) -> bool {
        !self.use_default_mute_for && self.mute_for > 0
    }

    pub fn is_muted_forever(&self) -> bool {
        self.is_muted() && self.mute_for > MUTE_FOREVER_AFTER_SECONDS
    }
}

/// `notificationSound` (TDLib 1.8.67, line 8857): "Describes a notification
/// sound in MP3 format".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationSound {
    pub id: i64,
    pub duration: i32,
    pub date: i32,
    pub title: String,
    pub data: String,
    /// `sound:file` — downloaded on demand with `downloadFile` when a
    /// notification needs it.
    pub sound: ParsedFile,
}

/// `NotificationSettingsScope` (TDLib 1.8.67, lines 3337–3343).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationSettingsScope {
    PrivateChats,
    GroupChats,
    ChannelChats,
}

impl NotificationSettingsScope {
    /// The `@type` constructor name for `getScopeNotificationSettings` /
    /// `setScopeNotificationSettings` requests.
    pub fn type_name(&self) -> &'static str {
        match self {
            NotificationSettingsScope::PrivateChats => "notificationSettingsScopePrivateChats",
            NotificationSettingsScope::GroupChats => "notificationSettingsScopeGroupChats",
            NotificationSettingsScope::ChannelChats => "notificationSettingsScopeChannelChats",
        }
    }

    /// Human label for the scope-defaults settings UI.
    pub fn label(&self) -> &'static str {
        match self {
            NotificationSettingsScope::PrivateChats => "Private chats",
            NotificationSettingsScope::GroupChats => "Groups",
            NotificationSettingsScope::ChannelChats => "Channels",
        }
    }

    /// All three scopes, in UI order.
    pub const ALL: [NotificationSettingsScope; 3] = [
        NotificationSettingsScope::PrivateChats,
        NotificationSettingsScope::GroupChats,
        NotificationSettingsScope::ChannelChats,
    ];
}

/// `scopeNotificationSettings` (TDLib 1.8.67, line 3375): defaults applied
/// when a chat's `chatNotificationSettings` keeps a `use_default_*` flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeNotificationSettings {
    pub mute_for: i32,
    /// 0 = disabled; -1 = app-dependent default sound (schema line 3368).
    pub sound_id: i64,
    pub show_preview: bool,
    pub use_default_mute_stories: bool,
    pub mute_stories: bool,
    pub story_sound_id: i64,
    pub show_story_poster: bool,
    pub disable_pinned_message_notifications: bool,
    pub disable_mention_notifications: bool,
}

impl Default for ScopeNotificationSettings {
    fn default() -> Self {
        Self {
            mute_for: 0,
            sound_id: -1,
            show_preview: true,
            use_default_mute_stories: true,
            mute_stories: false,
            story_sound_id: -1,
            show_story_poster: true,
            disable_pinned_message_notifications: false,
            disable_mention_notifications: false,
        }
    }
}

/// `message.forward_info.origin` (TDLib 1.8.67 `MessageOrigin`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageOrigin {
    User {
        user_id: UserId,
    },
    HiddenUser {
        sender_name: String,
    },
    Chat {
        chat_id: ChatId,
        author_signature: String,
    },
    Channel {
        chat_id: ChatId,
        message_id: MessageId,
        author_signature: String,
    },
}

/// Typed `messageForwardInfo`. `source` (Saved Messages / Replies) stays out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageForwardInfo {
    pub origin: MessageOrigin,
    pub date: i32,
}

/// Official Telegram default emoji reactions (tdesktop active-emoji first
/// row / Unigram default picker). Custom emoji and paid stay out of this slice.
pub const DEFAULT_EMOJI_REACTIONS: &[&str] = &[
    "👍", "👎", "❤", "🔥", "🥰", "👏", "😁", "🤔", "🤯", "😱", "🤬", "😢", "🎉", "🤩", "🤮", "💩",
];

/// `ReactionType` (TDLib 1.8.67). Phase 1 chips use emoji only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReactionType {
    Emoji { emoji: String },
    CustomEmoji { custom_emoji_id: i64 },
    Paid,
    Unknown,
}

impl ReactionType {
    pub fn emoji(emoji: impl Into<String>) -> Self {
        Self::Emoji {
            emoji: emoji.into(),
        }
    }

    pub fn emoji_text(&self) -> Option<&str> {
        match self {
            Self::Emoji { emoji } => Some(emoji.as_str()),
            _ => None,
        }
    }

    /// JSON object for `addMessageReaction` / `removeMessageReaction`.
    pub fn to_tdlib_json(&self) -> serde_json::Value {
        match self {
            Self::Emoji { emoji } => serde_json::json!({
                "@type": "reactionTypeEmoji",
                "emoji": emoji
            }),
            Self::CustomEmoji { custom_emoji_id } => serde_json::json!({
                "@type": "reactionTypeCustomEmoji",
                "custom_emoji_id": custom_emoji_id.to_string()
            }),
            Self::Paid => serde_json::json!({ "@type": "reactionTypePaid" }),
            Self::Unknown => serde_json::json!({ "@type": "reactionTypeEmoji", "emoji": "" }),
        }
    }
}

/// `messageReaction` (TDLib 1.8.67). `used_sender_id` / recent senders stay out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageReaction {
    pub reaction_type: ReactionType,
    pub total_count: i32,
    pub is_chosen: bool,
}

impl MessageReaction {
    /// Chip label: emoji + count (tdesktop InlineList / Unigram ReactionButton).
    pub fn chip_label(&self) -> Option<String> {
        let emoji = self.reaction_type.emoji_text()?;
        Some(format!("{emoji} {}", self.total_count))
    }
}

/// `messageReactions` (TDLib 1.8.67). Tags / paid reactors stay out of display.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MessageReactions {
    pub reactions: Vec<MessageReaction>,
    pub are_tags: bool,
}

impl MessageReactions {
    pub fn emoji_chips(&self) -> impl Iterator<Item = &MessageReaction> {
        self.reactions
            .iter()
            .filter(|reaction| reaction.reaction_type.emoji_text().is_some())
    }

    pub fn chosen_emoji(&self, emoji: &str) -> bool {
        self.reactions.iter().any(|reaction| {
            reaction.is_chosen && reaction.reaction_type.emoji_text() == Some(emoji)
        })
    }
}

/// `messageInteractionInfo` (TDLib 1.8.67). `reply_info` stays out of this slice.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MessageInteractionInfo {
    pub view_count: i32,
    pub forward_count: i32,
    pub reactions: Option<MessageReactions>,
}

impl MessageInteractionInfo {
    pub fn emoji_chips(&self) -> Vec<&MessageReaction> {
        self.reactions
            .as_ref()
            .map(|reactions| reactions.emoji_chips().collect())
            .unwrap_or_default()
    }

    pub fn chosen_emoji(&self, emoji: &str) -> bool {
        self.reactions
            .as_ref()
            .is_some_and(|reactions| reactions.chosen_emoji(emoji))
    }
}

/// Toggle the current user's chosen emoji (tdesktop chip click / Unigram
/// ReactionButton). Used to build the next `updateMessageInteractionInfo`.
pub fn toggle_chosen_emoji_reaction(
    current: Option<&MessageInteractionInfo>,
    emoji: &str,
) -> MessageInteractionInfo {
    let mut info = current.cloned().unwrap_or_default();
    let mut reactions = info.reactions.take().unwrap_or_default();
    if let Some(existing) = reactions
        .reactions
        .iter_mut()
        .find(|reaction| reaction.reaction_type.emoji_text() == Some(emoji))
    {
        if existing.is_chosen {
            existing.is_chosen = false;
            existing.total_count = (existing.total_count - 1).max(0);
        } else {
            existing.is_chosen = true;
            existing.total_count += 1;
        }
    } else {
        reactions.reactions.push(MessageReaction {
            reaction_type: ReactionType::emoji(emoji),
            total_count: 1,
            is_chosen: true,
        });
    }
    reactions
        .reactions
        .retain(|reaction| reaction.total_count > 0);
    info.reactions = if reactions.reactions.is_empty() {
        None
    } else {
        Some(reactions)
    };
    info
}

/// Same-chat / known-chat reply metadata from `message.reply_to`.
/// Stories and unknown `MessageReplyTo` variants are dropped (out of scope).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageReplyTo {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub quote_text: Option<String>,
    pub content_preview: Option<String>,
}

impl MessageReplyTo {
    pub fn is_same_chat(&self, open_chat: ChatId) -> bool {
        self.chat_id.0 == 0 || self.chat_id == open_chat
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMessage {
    pub id: MessageId,
    pub chat_id: ChatId,
    pub is_outgoing: bool,
    /// Schema `message.is_pinned` (TDLib 1.8.67).
    pub is_pinned: bool,
    /// Schema `message.topic_id` (TDLib 1.8.67, line 3165): the
    /// `forum_topic_id` when the topic is `messageTopicForum`, `None` for
    /// every other `MessageTopic` variant (threads, direct messages, saved
    /// messages) and when the field is absent. Parity slice 4 routes topic
    /// messages into the topic's history.
    pub topic_id: Option<i32>,
    /// Schema `message.media_album_id` (int64). `0` means the message is not in an album.
    pub media_album_id: i64,
    pub content: MessageContent,
    pub files: Vec<ParsedFile>,
    pub reply_to: Option<MessageReplyTo>,
    pub forward_info: Option<MessageForwardInfo>,
    pub interaction_info: Option<MessageInteractionInfo>,
    /// Schema `message.reply_markup` (TDLib 1.8.67). Only
    /// `replyMarkupInlineKeyboard` is kept; other markups are `None`.
    pub reply_markup: Option<InlineKeyboard>,
    /// Phase B3: `message.self_destruct_type` / `message.self_destruct_in`
    /// (TDLib 1.8.67, `schema/td_api.tl:3146`–`:3147` / `:3165`).
    /// `messageSelfDestructTypeTimer` (line 5915) /
    /// `messageSelfDestructTypeImmediately` (line 5918); unknown future
    /// variants degrade to `None` (message renders without a timer badge).
    pub self_destruct: Option<MessageSelfDestruct>,
}

/// Phase B3: the configured self-destruct mode of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfDestructKind {
    /// `messageSelfDestructTypeTimer` — destroyed `secs` seconds after the
    /// content was opened (schema 1.8.67 line 5915).
    Timer { secs: i32 },
    /// `messageSelfDestructTypeImmediately` — destroyed once closed after a
    /// single viewing (schema 1.8.67 line 5918).
    Immediately,
}

/// Phase B3: parsed `message.self_destruct_type` + `message.self_destruct_in`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageSelfDestruct {
    pub kind: SelfDestructKind,
    /// Latest `message.self_destruct_in`, converted to whole milliseconds.
    /// `0` means the destruction isn't scheduled yet (the content hasn't
    /// been opened; schema 1.8.67 line 3147). TDLib emits
    /// `updateDeleteMessages` when the timer fires, so the row disappears
    /// through the normal delete path — no special handling needed.
    /// Whole milliseconds (not `f64` seconds) so the struct keeps the
    /// `Eq` derive that `ParsedMessage` / `HistoryMessage` require.
    pub expires_in_ms: i64,
    /// Local clock (ms) when `expires_in_ms` was received, for the local
    /// countdown decay (`slow_mode_delay_expires_in` pattern, Phase A1).
    pub fetched_at_ms: u64,
}

impl MessageSelfDestruct {
    /// Locally decayed whole seconds left, or `None` when destruction isn't
    /// scheduled yet. The value keeps decaying to 0 (the row stays until
    /// TDLib's `updateDeleteMessages` removes it).
    pub fn remaining_secs(&self, now_ms: u64) -> Option<u64> {
        if self.expires_in_ms <= 0 {
            return None;
        }
        let elapsed_ms = now_ms.saturating_sub(self.fetched_at_ms) as i64;
        let remaining_ms = self.expires_in_ms - elapsed_ms;
        Some(if remaining_ms > 0 {
            ((remaining_ms + 999) / 1000) as u64
        } else {
            0
        })
    }

    /// Timer badge for media rows: "⏱ view once" / "⏱ 60s" (not scheduled
    /// yet) / "⏱ 42s left" (scheduled).
    pub fn badge_label(&self, now_ms: u64) -> String {
        match self.kind {
            SelfDestructKind::Immediately => "⏱ view once".to_string(),
            SelfDestructKind::Timer { secs } => match self.remaining_secs(now_ms) {
                Some(left) => format!("⏱ {left}s left"),
                None => format!("⏱ {secs}s"),
            },
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Parse `message.self_destruct_type` + `message.self_destruct_in` (schema
/// 1.8.67 lines 3146–3147 / 3165 / 5915 / 5918). `self_destruct_in` arrives
/// as a double (seconds, possibly fractional); non-finite or negative
/// values are treated as unscheduled.
fn parse_self_destruct(
    type_value: Option<&Value>,
    in_value: Option<&Value>,
) -> Option<MessageSelfDestruct> {
    let type_value = type_value?;
    if type_value.is_null() {
        return None;
    }
    let kind = match type_value.get("@type").and_then(Value::as_str) {
        Some("messageSelfDestructTypeTimer") => SelfDestructKind::Timer {
            secs: type_value
                .get("self_destruct_time")
                .and_then(Value::as_i64)
                .map(|s| s.clamp(0, i32::MAX as i64) as i32)
                .unwrap_or(0),
        },
        Some("messageSelfDestructTypeImmediately") => SelfDestructKind::Immediately,
        _ => return None,
    };
    let expires_in_ms = in_value
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| (v * 1000.0).round() as i64)
        .unwrap_or(0);
    Some(MessageSelfDestruct {
        kind,
        expires_in_ms,
        fetched_at_ms: now_ms(),
    })
}

/// One `forumTopic` (TDLib 1.8.67, `schema/td_api.tl:3968` + `forumTopicInfo`
/// at 3953). Only the fields the topic list / topic view need are kept;
/// dropped fields are documented in the Phase 5.1 DECISIONS entry:
/// `icon` (custom-emoji topic icons are not rendered), `creation_date`,
/// `creator_id`, `is_outgoing`, `is_hidden`, `is_name_implicit`,
/// `last_read_inbox/outbox_message_id`, `unread_mention_count`,
/// `unread_reaction_count`, `unread_poll_vote_count`,
/// `notification_settings`, `draft_message`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForumTopic {
    pub forum_topic_id: i32,
    pub name: String,
    pub is_general: bool,
    pub is_closed: bool,
    pub is_pinned: bool,
    pub unread_count: i32,
    /// Schema `forumTopic.order` — topics sort by order descending.
    pub order: i64,
    /// Cheap preview of `forumTopic.last_message` via
    /// `MessageContent::preview`; empty when there is no last message.
    pub last_message_preview: String,
}

/// Parse one `forumTopic` object. Returns `None` when `info` is missing or
/// malformed (the row is skipped, matching the lenient message parsing).
fn parse_forum_topic(value: &Value) -> Option<ForumTopic> {
    let info = value.get("info")?;
    let forum_topic_id = info.get("forum_topic_id")?.as_i64()? as i32;
    let name = json_field_str(info, "name");
    let is_general = info
        .get("is_general")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_closed = info
        .get("is_closed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_pinned = value
        .get("is_pinned")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let unread_count = value
        .get("unread_count")
        .and_then(Value::as_i64)
        .unwrap_or(0) as i32;
    let order = int53_or_zero(value.get("order"));
    let last_message_preview = value
        .get("last_message")
        .and_then(|m| parse_message(m).ok())
        .map(|m| m.content.preview())
        .unwrap_or_default();
    Some(ForumTopic {
        forum_topic_id,
        name,
        is_general,
        is_closed,
        is_pinned,
        unread_count,
        order,
        last_message_preview,
    })
}

/// Phase 9.1: a story list identifier — `storyListMain` /
/// `storyListArchive` (TDLib 1.8.67, `schema/td_api.tl:6684-6690`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryListView {
    Main,
    Archive,
}

/// Phase 9.1: `storyInfo` — basic information about one active story
/// (TDLib 1.8.67, `schema/td_api.tl:6767-6773`). `chatActiveStories.stories`
/// arrive in chronological order (increasing `story_id`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryInfoView {
    pub story_id: i32,
    pub date: i32,
    pub is_for_close_friends: bool,
    pub is_live: bool,
}

/// Phase 9.1: `chatActiveStories` — active stories posted by a chat (TDLib
/// 1.8.67, `schema/td_api.tl:6776-6783`). Only the tray needs are kept:
/// `list` (null when the stories are not shown in any story list),
/// `order` (tray sort key), `max_read_story_id` (unread rings), and the
/// `storyInfo` list. Dropped: `can_be_archived`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatActiveStoriesView {
    pub chat_id: i64,
    pub list: Option<StoryListView>,
    pub order: i64,
    pub max_read_story_id: i32,
    pub stories: Vec<StoryInfoView>,
}

impl ChatActiveStoriesView {
    /// True when any active story is newer than `max_read_story_id` — the
    /// tray ring renders unread.
    pub fn has_unread(&self) -> bool {
        self.stories
            .iter()
            .any(|story| story.story_id > self.max_read_story_id)
    }
}

/// Phase 9.1: story media Quill renders in the viewer: photo and video only.
/// Live stories and unsupported content keep the story item but render a
/// placeholder (no group-call join, no RTMP — out of scope).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoryContentView {
    Photo {
        sizes: Vec<PhotoSizeView>,
    },
    Video {
        /// `storyVideo.thumbnail.file` (`thumbnail`, TDLib 1.8.67,
        /// `schema/td_api.tl:6633`) — the only display candidate; the full
        /// clip is not renderable by the image element (same call as the
        /// Phase 4.5 media viewer).
        thumb_file_id: Option<FileId>,
        thumb_width: i32,
        thumb_height: i32,
        /// `storyVideo.duration:double` — whole seconds for the label.
        duration_secs: i32,
        /// `storyVideo.video:file` — downloaded when there is no thumbnail
        /// so the file lands local.
        file_id: FileId,
    },
    Live,
    Unsupported,
}

/// Phase 9.2: `storyInteractionInfo` — interaction counters on a story
/// (TDLib 1.8.67, `schema/td_api.tl:6712`). Only populated by TDLib for
/// stories the current user posted (`story.can_get_interactions`); kept so
/// the viewer can render view/reaction counts on own stories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StoryInteractionInfoView {
    pub view_count: i32,
    pub forward_count: i32,
    pub reaction_count: i32,
}

impl StoryInteractionInfoView {
    /// True when at least one counter is nonzero.
    pub fn any_nonzero(&self) -> bool {
        self.view_count > 0 || self.forward_count > 0 || self.reaction_count > 0
    }
}

/// Phase 9.2: one emoji reaction the story picker can offer —
/// `availableReaction` (TDLib 1.8.67, `schema/td_api.tl:7321`). Only
/// `reactionTypeEmoji` entries render; custom-emoji entries are dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryAvailableReactionView {
    pub emoji: String,
    pub needs_premium: bool,
}

/// Phase 9.1: `story` — the full story object (TDLib 1.8.67,
/// `schema/td_api.tl:6742`). Kept: ids, `date`, `content`, `caption`;
/// Phase 9.2 keeps: `chosen_reaction_type` (the user's own reaction),
/// `interaction_info` (view/forward/reaction counts), and the
/// `can_be_deleted` / `can_be_replied` / `can_get_interactions` gates.
/// Dropped (see DECISIONS.md Phase 9.1): repost info, privacy settings,
/// clickable areas, album ids, and the other `is_*` / `can_be_*` flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedStory {
    pub id: i32,
    pub poster_chat_id: i64,
    pub date: i32,
    pub content: StoryContentView,
    pub caption: String,
    pub caption_entities: Vec<TextEntity>,
    /// Phase 9.2: the user's own reaction on this story (`emoji`), `None`
    /// when none is chosen or when the chosen reaction is a custom emoji /
    /// paid reaction (same call as message reactions).
    pub chosen_reaction_emoji: Option<String>,
    /// Phase 9.2: `storyInteractionInfo` — counters, meaningful only when
    /// `can_get_interactions`.
    pub interaction_info: Option<StoryInteractionInfoView>,
    /// Phase 9.2: `story.can_be_deleted` — gates the viewer Delete button
    /// (`deleteStory`, `schema/td_api.tl:13754`).
    pub can_be_deleted: bool,
    /// Phase 9.2: `story.can_be_replied` — gates the viewer Reply affordance
    /// (`inputMessageReplyToStory`, `schema/td_api.tl:3099`).
    pub can_be_replied: bool,
    /// Phase 9.2: `story.can_get_interactions` — the interaction counters are
    /// the user's own.
    pub can_get_interactions: bool,
}

fn parse_story_list(value: Option<&Value>) -> Option<StoryListView> {
    match value
        .and_then(|value| value.get("@type"))
        .and_then(Value::as_str)
    {
        Some("storyListMain") => Some(StoryListView::Main),
        Some("storyListArchive") => Some(StoryListView::Archive),
        _ => None,
    }
}

fn parse_story_info(value: &Value) -> Option<StoryInfoView> {
    Some(StoryInfoView {
        story_id: value.get("story_id")?.as_i64()? as i32,
        date: value.get("date").and_then(Value::as_i64).unwrap_or(0) as i32,
        is_for_close_friends: value
            .get("is_for_close_friends")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_live: value
            .get("is_live")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn parse_chat_active_stories(value: &Value) -> Option<ChatActiveStoriesView> {
    Some(ChatActiveStoriesView {
        chat_id: int53(value.get("chat_id")).ok()?,
        list: parse_story_list(value.get("list")),
        order: int53_or_zero(value.get("order")),
        max_read_story_id: value
            .get("max_read_story_id")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        stories: value
            .get("stories")
            .and_then(Value::as_array)
            .map(|stories| stories.iter().filter_map(parse_story_info).collect())
            .unwrap_or_default(),
    })
}

/// Parse one `story` object. Returns `None` when the required ids are
/// missing; unknown or missing `content` degrades to `Unsupported` rather
/// than failing the row. Collects the content's `file`s like the message
/// content parsers do.
fn parse_story(value: &Value) -> Option<(ParsedStory, Vec<ParsedFile>)> {
    let id = value.get("id")?.as_i64()? as i32;
    let poster_chat_id = int53(value.get("poster_chat_id")).ok()?;
    let (caption, caption_entities) = parse_caption(value.get("caption"));
    let mut files = Vec::new();
    let content = parse_story_content(value.get("content"), &mut files);
    files.retain(|file| file.id.0 != 0);
    Some((
        ParsedStory {
            id,
            poster_chat_id,
            date: value.get("date").and_then(Value::as_i64).unwrap_or(0) as i32,
            content,
            caption,
            caption_entities,
            chosen_reaction_emoji: parse_story_chosen_reaction(value.get("chosen_reaction_type")),
            interaction_info: parse_story_interaction_info(value.get("interaction_info")),
            can_be_deleted: value
                .get("can_be_deleted")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            can_be_replied: value
                .get("can_be_replied")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            can_get_interactions: value
                .get("can_get_interactions")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        files,
    ))
}

/// Phase 9.2: `chosen_reaction_type` on a `story` — returns the emoji when
/// the user's chosen reaction is a `reactionTypeEmoji`, else `None` (no
/// reaction, custom emoji, or paid reaction).
fn parse_story_chosen_reaction(value: Option<&Value>) -> Option<String> {
    let reaction_type = value.filter(|value| !value.is_null())?;
    if reaction_type.get("@type").and_then(Value::as_str) != Some("reactionTypeEmoji") {
        return None;
    }
    reaction_type
        .get("emoji")
        .and_then(Value::as_str)
        .filter(|emoji| !emoji.is_empty())
        .map(str::to_string)
}

/// Phase 9.2: `storyInteractionInfo` counters; `None` when the field is
/// missing or null.
fn parse_story_interaction_info(value: Option<&Value>) -> Option<StoryInteractionInfoView> {
    let info = value.filter(|value| !value.is_null())?;
    Some(StoryInteractionInfoView {
        view_count: info.get("view_count").and_then(Value::as_i64).unwrap_or(0) as i32,
        forward_count: info
            .get("forward_count")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        reaction_count: info
            .get("reaction_count")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
    })
}

/// Phase 9.2: one `availableReaction` row into the picker shape; drops
/// non-emoji reactions (custom emoji previews stay out of this slice).
fn parse_story_available_reaction(value: &Value) -> Option<StoryAvailableReactionView> {
    let reaction = value.get("type")?;
    if reaction.get("@type").and_then(Value::as_str) != Some("reactionTypeEmoji") {
        return None;
    }
    let emoji = reaction
        .get("emoji")
        .and_then(Value::as_str)
        .filter(|emoji| !emoji.is_empty())?
        .to_string();
    Some(StoryAvailableReactionView {
        emoji,
        needs_premium: value
            .get("needs_premium")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn parse_story_content(value: Option<&Value>, files: &mut Vec<ParsedFile>) -> StoryContentView {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return StoryContentView::Unsupported;
    };
    match value.get("@type").and_then(Value::as_str) {
        Some("storyContentPhoto") => match value.get("photo") {
            Some(photo) => {
                let (sizes, mut photo_files) = parse_photo_sizes(photo);
                files.append(&mut photo_files);
                StoryContentView::Photo { sizes }
            }
            None => StoryContentView::Unsupported,
        },
        Some("storyContentVideo") => {
            let video = value.get("video");
            let duration_secs = video
                .and_then(|video| video.get("duration"))
                .and_then(|duration| {
                    duration
                        .as_f64()
                        .or_else(|| duration.as_i64().map(|d| d as f64))
                })
                .unwrap_or(0.0) as i32;
            let file_id = video
                .and_then(|video| parse_file(video.get("video")).ok())
                .map(|file| {
                    let id = file.id;
                    files.push(file);
                    id
                })
                .unwrap_or(FileId(0));
            let thumb = video
                .and_then(|video| video.get("thumbnail"))
                .filter(|thumb| !thumb.is_null());
            let (thumb_file_id, thumb_width, thumb_height) = match thumb {
                Some(thumb) => (
                    parse_file(thumb.get("file"))
                        .ok()
                        .map(|file| {
                            let id = file.id;
                            files.push(file);
                            id
                        })
                        .filter(|id| id.0 != 0),
                    int53_or_zero(thumb.get("width")) as i32,
                    int53_or_zero(thumb.get("height")) as i32,
                ),
                None => (None, 0, 0),
            };
            StoryContentView::Video {
                thumb_file_id,
                thumb_width,
                thumb_height,
                duration_secs,
                file_id,
            }
        }
        Some("storyContentLive") => StoryContentView::Live,
        _ => StoryContentView::Unsupported,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageContent {
    Text(TextContent),
    Photo(PhotoContent),
    Document(DocumentContent),
    Sticker(StickerContent),
    Animation(AnimationContent),
    Video(VideoContent),
    VideoNote(VideoNoteContent),
    VoiceNote(VoiceNoteContent),
    Audio(AudioContent),
    /// Phase 4.2: `messagePoll` (TDLib 1.8.67, `schema/td_api.tl:5241`).
    Poll(PollContent),
    /// Phase 4.3: `messageLocation` (TDLib 1.8.67, `schema/td_api.tl:5214`)
    /// and `messageLiveLocation` (`schema/td_api.tl:5211`). The latter
    /// carries `LiveLocation` state; the former sets `live: None`.
    Location(LocationContent),
    /// Phase 4.3: `messageVenue` (TDLib 1.8.67, `schema/td_api.tl:5217`).
    Venue(VenueContent),
    /// Phase 4.3: `messageContact` (TDLib 1.8.67, `schema/td_api.tl:5220`).
    Contact(ContactContent),
    /// Phase 4.4: `messageDice` (TDLib 1.8.67, `schema/td_api.tl:5231`).
    Dice(DiceContent),
    Unsupported {
        type_name: String,
    },
}

/// `formattedText` plus optional `messageText.link_preview` (TDLib 1.8.67).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextContent {
    pub text: String,
    pub entities: Vec<TextEntity>,
    pub link_preview: Option<LinkPreview>,
}

impl TextContent {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            entities: Vec::new(),
            link_preview: None,
        }
    }
}

impl From<&str> for TextContent {
    fn from(text: &str) -> Self {
        Self::plain(text)
    }
}

impl From<String> for TextContent {
    fn from(text: String) -> Self {
        Self::plain(text)
    }
}

/// `pollOption` (TDLib 1.8.67, `schema/td_api.tl:456`). `media`,
/// `recent_voter_ids`, `is_being_chosen`, `author`, and `addition_date` are
/// not kept — Quill renders the bar, the count, and the chosen mark only.
/// Note: `id` is a string identifier for the option, while `setPollAnswer`
/// takes 0-based **indexes** into `poll.options` (schema line 12932).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollOption {
    pub id: String,
    pub text: String,
    pub voter_count: i32,
    pub vote_percentage: i32,
    pub is_chosen: bool,
}

/// `pollType` (TDLib 1.8.67, `schema/td_api.tl:468` / `:475`).
/// `pollTypeQuiz.explanation` and `explanation_media` are not kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollType {
    Regular,
    Quiz { correct_option_ids: Vec<i32> },
}

/// `poll` (TDLib 1.8.67, `schema/td_api.tl:711`). `recent_voter_ids`,
/// `can_get_voters`, `can_see_results`, `members_only`, `country_codes`,
/// `option_order`, `open_period`, `close_date`, and `vote_restriction_reason`
/// are not kept in this slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Poll {
    pub id: i64,
    pub question: String,
    pub options: Vec<PollOption>,
    pub total_voter_count: i32,
    pub is_anonymous: bool,
    pub allows_multiple_answers: bool,
    pub allows_revoting: bool,
    pub is_closed: bool,
    pub poll_type: PollType,
}

impl Poll {
    /// 0-based indexes of the options currently marked chosen — the values
    /// `setPollAnswer` expects (schema line 12932).
    pub fn chosen_indexes(&self) -> Vec<i32> {
        self.options
            .iter()
            .enumerate()
            .filter(|(_, option)| option.is_chosen)
            .map(|(index, _)| index as i32)
            .collect()
    }

    /// The poll can receive a vote from the user.
    pub fn can_vote(&self) -> bool {
        !self.is_closed
    }
}

/// `messagePoll` (TDLib 1.8.67, `schema/td_api.tl:5241`). `media` is not
/// rendered in this slice (photo/document/… attachments on polls stay out).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollContent {
    pub poll: Poll,
    pub description: String,
}

/// `location` (TDLib 1.8.67, `schema/td_api.tl:646`): `latitude` /
/// `longitude` in degrees, `horizontal_accuracy` in meters (0 = unknown).
/// Phase 4.3: coordinates are stored as integer **microdegrees**
/// (`lat_e6` / `lon_e6`, 10⁻⁶ degrees ≈ 11 cm) so the parsed model keeps
/// the `Eq` derive used across the envelope types — more than enough for
/// display and map-link generation. Accuracy is rounded to whole meters
/// (`accuracy_m`; 0 = unknown, per schema).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeoLocation {
    pub lat_e6: i64,
    pub lon_e6: i64,
    pub accuracy_m: i32,
}

impl GeoLocation {
    /// Latitude in degrees.
    pub fn latitude(&self) -> f64 {
        self.lat_e6 as f64 / 1e6
    }

    /// Longitude in degrees.
    pub fn longitude(&self) -> f64 {
        self.lon_e6 as f64 / 1e6
    }

    /// User-facing coordinate line, e.g. `37.7749, -122.4194`.
    pub fn coords_label(&self) -> String {
        format!("{:.4}, {:.4}", self.latitude(), self.longitude())
    }

    /// OpenStreetMap deep link for the "Open map" row action. The
    /// coordinates come from the parsed message, so they contain no
    /// whitespace or control characters and the `https://` scheme passes
    /// `platform::open_external_url`'s scheme gate.
    pub fn open_street_map_url(&self) -> String {
        format!(
            "https://www.openstreetmap.org/?mlat={:.6}&mlon={:.6}",
            self.latitude(),
            self.longitude()
        )
    }
}

/// Safe rule for coordinate parsing (Phase 4.3, documented per
/// `messageLocation` / `messageVenue`): `latitude` and `longitude` must be
/// finite numbers with `|lat| <= 90` and `|lon| <= 180`. Anything else —
/// NaN, infinities, or out-of-range degrees — means corrupt data, and the
/// location is dropped entirely (the message renders as `Unsupported`)
/// rather than pinned to a clamped pole or fed to a map link.
fn geo_location(value: Option<&Value>) -> Option<GeoLocation> {
    let value = value?;
    let latitude = value.get("latitude").and_then(Value::as_f64)?;
    let longitude = value.get("longitude").and_then(Value::as_f64)?;
    if !latitude.is_finite() || !longitude.is_finite() {
        return None;
    }
    if latitude.abs() > 90.0 || longitude.abs() > 180.0 {
        return None;
    }
    let accuracy = value
        .get("horizontal_accuracy")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let accuracy_m = if accuracy.is_finite() && accuracy > 0.0 {
        accuracy.round().clamp(0.0, i32::MAX as f64) as i32
    } else {
        0
    };
    Some(GeoLocation {
        lat_e6: (latitude * 1e6).round() as i64,
        lon_e6: (longitude * 1e6).round() as i64,
        accuracy_m,
    })
}

/// `liveLocation` (TDLib 1.8.67, `schema/td_api.tl:653`): live-period state
/// attached to a location. `live_period` is relative to the message send
/// date in seconds (`0x7FFFFFFF` = updates forever); `heading` is 1–360
/// degrees (0 = unknown); `proximity_alert_radius` is 0–100000 meters
/// (0 = disabled).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveLocationState {
    pub live_period: i32,
    pub expires_in: i32,
    pub heading: i32,
    pub proximity_alert_radius: i32,
}

impl LiveLocationState {
    /// Static status line; live re-rendering is out of this slice, so the
    /// remaining time is a snapshot from `expires_in` at parse time.
    pub fn status_label(&self) -> String {
        if self.expires_in <= 0 {
            return "Live location ended".into();
        }
        let mut parts = vec![format!(
            "Live · expires in {}",
            duration_label(self.expires_in)
        )];
        if self.heading > 0 {
            parts.push(format!("heading {}°", self.heading));
        }
        if self.proximity_alert_radius > 0 {
            parts.push(format!(
                "proximity alert ≤ {}",
                meters_label(self.proximity_alert_radius)
            ));
        }
        parts.join(" · ")
    }
}

/// `mm:ss` or `Xh Ym` for `expires_in`-style second counts.
fn duration_label(seconds: i32) -> String {
    let seconds = seconds.max(0) as i64;
    if seconds >= 3600 {
        format!("{}h {:02}m", seconds / 3600, (seconds % 3600) / 60)
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}

fn meters_label(meters: i32) -> String {
    if meters >= 1000 {
        format!("{:.1} km", meters as f64 / 1000.0)
    } else {
        format!("{meters} m")
    }
}

/// `messageLocation` (`schema/td_api.tl:5214`) / `messageLiveLocation`
/// (`schema/td_api.tl:5211`, `expires_in` = seconds left for updates,
/// 0 = can't be updated anymore). For `messageLocation`, `live` is
/// `None`; for `messageLiveLocation`, it carries the live state. Note the
/// schema split: `messageLocation` itself carries **no** live fields — the
/// task's `live_period` / `heading` / `proximity_alert_radius` live on
/// `liveLocation` (`schema/td_api.tl:653`), which is why both constructors
/// are parsed here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocationContent {
    pub location: GeoLocation,
    pub live: Option<LiveLocationState>,
}

/// `venue` (TDLib 1.8.67, `schema/td_api.tl:663`). `id` and `type` are
/// provider-database identifiers and are not kept — Quill renders the
/// human-readable `title` + `address` and the map link from `location`.
/// `provider` (e.g. "foursquare", "gplaces") is kept for the subtitle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VenueContent {
    pub location: GeoLocation,
    pub title: String,
    pub address: String,
    pub provider: String,
}

/// `contact` (TDLib 1.8.67, `schema/td_api.tl:640`). `vcard` (raw vCard
/// data, up to 2048 bytes) is kept for future address-book use but not
/// rendered in this slice; `user_id` is 0 when unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactContent {
    pub phone_number: String,
    pub first_name: String,
    pub last_name: String,
    pub vcard: String,
    pub user_id: i64,
}

impl ContactContent {
    /// `first_name` + `last_name`, trimmed; empty when both are empty.
    pub fn display_name(&self) -> String {
        let name = format!("{} {}", self.first_name.trim(), self.last_name.trim());
        name.trim().to_string()
    }
}

/// `messageDice` (TDLib 1.8.67, `schema/td_api.tl:5231`). `emoji` is the
/// dice glyph sent (🎲, 🎯, 🏀, ⚽, 🎰, 🎳) and `value` is the rolled
/// number (its range depends on the emoji, e.g. 1–6 for 🎲, 1–64 for 🎰).
/// Dropped fields (documented): `initial_state` / `final_state`
/// (`DiceStickers` animated stickers — the roll animation is out of scope
/// in this slice) and `success_animation_frame_number` (the frame where a
/// "success" animation starts; only used by the animated rendering).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiceContent {
    pub emoji: String,
    pub value: i32,
}

impl DiceContent {
    /// The glyph to render. An empty `emoji` (TDLib should always send
    /// one, but malformed payloads happen) falls back to the plain die
    /// rather than rendering nothing.
    pub fn face(&self) -> &str {
        if self.emoji.is_empty() {
            "🎲"
        } else {
            &self.emoji
        }
    }

    /// Short line for chat-list previews and the composer reply target,
    /// e.g. `🎲 4`.
    pub fn label(&self) -> String {
        format!("{} {}", self.face(), self.value)
    }
}

/// `linkPreview` card. Photo comes from `type` when that constructor carries a `photo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkPreview {
    pub url: String,
    pub display_url: String,
    pub site_name: String,
    pub title: String,
    pub description: String,
    pub show_large_media: bool,
    pub show_media_above_description: bool,
    pub show_above_text: bool,
    pub photo: Option<PhotoContent>,
}

impl LinkPreview {
    pub fn has_card(&self) -> bool {
        !self.url.is_empty()
            || !self.site_name.is_empty()
            || !self.title.is_empty()
            || !self.description.is_empty()
            || self.photo.is_some()
    }
}

/// `advertisementSponsor` (TDLib 1.8.67): who backs a sponsored message.
/// `photo` is null when the sponsor must not show one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvertisementSponsor {
    pub url: String,
    pub photo: Option<PhotoContent>,
    pub info: String,
}

/// `sponsoredMessage` (TDLib 1.8.67). Content is text, animation, photo, or
/// video per the schema; `accent_color_id` / `background_custom_emoji_id` are
/// not rendered in this slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SponsoredMessage {
    /// int53; unique for the chat among both ordinary and sponsored messages.
    pub message_id: i64,
    pub is_recommended: bool,
    pub can_be_reported: bool,
    pub content: MessageContent,
    pub sponsor: AdvertisementSponsor,
    pub title: String,
    pub button_text: String,
    pub additional_info: String,
}

impl SponsoredMessage {
    /// "Recommended" when `is_recommended`, otherwise "Sponsored" (schema).
    pub fn kind_label(&self) -> &'static str {
        if self.is_recommended {
            "Recommended"
        } else {
            "Sponsored"
        }
    }

    /// Thumb file ids worth auto-downloading at priority 1 (content + sponsor).
    pub fn thumb_file_ids(&self) -> Vec<FileId> {
        let mut ids = Vec::new();
        match &self.content {
            MessageContent::Photo(photo) => {
                if let Some(size) = photo.thumb_size() {
                    ids.push(size.file_id);
                }
            }
            MessageContent::Animation(animation) => {
                if let Some(file_id) = animation.thumb_file_id() {
                    ids.push(file_id);
                }
            }
            MessageContent::Video(video) => {
                if let Some(file_id) = video.thumb_file_id() {
                    ids.push(file_id);
                }
            }
            _ => {}
        }
        if let Some(photo) = &self.sponsor.photo
            && let Some(size) = photo.thumb_size()
        {
            ids.push(size.file_id);
        }
        ids
    }
}

/// `reportOption` (TDLib 1.8.67). `id` is `bytes` (base64 in JSON); echoed
/// back into `reportChatSponsoredMessage`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportOption {
    pub id: String,
    pub text: String,
}

/// `ReportSponsoredResult` (TDLib 1.8.67): outcome of `reportChatSponsoredMessage`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportSponsoredResult {
    Ok,
    Failed,
    OptionRequired {
        title: String,
        options: Vec<ReportOption>,
    },
    AdsHidden,
    PremiumRequired,
}

impl ReportSponsoredResult {
    /// Short user-facing note (no TDLib text is echoed).
    pub fn user_message(&self) -> &'static str {
        match self {
            ReportSponsoredResult::Ok => "Report sent",
            ReportSponsoredResult::Failed => "Could not report this message",
            ReportSponsoredResult::OptionRequired { .. } => "Choose a report reason",
            ReportSponsoredResult::AdsHidden => "Sponsored messages hidden",
            ReportSponsoredResult::PremiumRequired => {
                "Hiding sponsored messages needs Telegram Premium"
            }
        }
    }
}

impl MessageContent {
    pub fn preview(&self) -> String {
        match self {
            MessageContent::Text(text) => text.text.chars().take(80).collect(),
            MessageContent::Photo(photo) if photo.caption.is_empty() => "Photo".into(),
            MessageContent::Photo(photo) => photo.caption.chars().take(80).collect(),
            MessageContent::Document(doc) if !doc.caption.is_empty() => {
                doc.caption.chars().take(80).collect()
            }
            MessageContent::Document(doc) if !doc.file_name.is_empty() => {
                doc.file_name.chars().take(80).collect()
            }
            MessageContent::Document(_) => "Document".into(),
            MessageContent::Sticker(sticker) if !sticker.emoji.is_empty() => sticker.emoji.clone(),
            MessageContent::Sticker(_) => "Sticker".into(),
            MessageContent::Animation(animation) if !animation.caption.is_empty() => {
                animation.caption.chars().take(80).collect()
            }
            MessageContent::Animation(_) => "GIF".into(),
            MessageContent::Video(video) if !video.caption.is_empty() => {
                video.caption.chars().take(80).collect()
            }
            MessageContent::Video(_) => "Video".into(),
            MessageContent::VideoNote(_) => "Video note".into(),
            MessageContent::VoiceNote(note) if !note.caption.is_empty() => {
                note.caption.chars().take(80).collect()
            }
            MessageContent::VoiceNote(_) => "Voice message".into(),
            MessageContent::Audio(audio) if !audio.caption.is_empty() => {
                audio.caption.chars().take(80).collect()
            }
            MessageContent::Audio(audio) if !audio.title.is_empty() => {
                audio.title.chars().take(80).collect()
            }
            MessageContent::Audio(audio) if !audio.file_name.is_empty() => {
                audio.file_name.chars().take(80).collect()
            }
            MessageContent::Audio(_) => "Audio".into(),
            MessageContent::Poll(poll) => {
                let question = poll.poll.question.trim();
                if question.is_empty() {
                    "Poll".into()
                } else {
                    question.chars().take(80).collect()
                }
            }
            MessageContent::Location(location) => {
                if location.live.is_some() {
                    "📍 Live location".into()
                } else {
                    "📍 Location".into()
                }
            }
            MessageContent::Venue(venue) => {
                let title = venue.title.trim();
                if title.is_empty() {
                    "📍 Venue".into()
                } else {
                    format!("📍 {}", title.chars().take(76).collect::<String>())
                }
            }
            MessageContent::Contact(contact) => {
                let name = contact.display_name();
                if name.is_empty() {
                    "👤 Contact".into()
                } else {
                    format!("👤 {}", name.chars().take(76).collect::<String>())
                }
            }
            MessageContent::Dice(dice) => dice.label(),
            MessageContent::Unsupported { type_name } => format!("({type_name})"),
        }
    }

    /// `updateMessageContentOpened` sets `messageVoiceNote.is_listened` and
    /// `messageVideoNote.is_viewed`.
    pub fn mark_content_opened(&mut self) {
        match self {
            MessageContent::VoiceNote(note) => note.is_listened = true,
            MessageContent::VideoNote(note) => note.is_viewed = true,
            _ => {}
        }
    }
}

/// `photo` + caption flags from `messagePhoto` (TDLib 1.8.67).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoContent {
    pub caption: String,
    /// Phase 4.1: entities for `caption` (same list as message text).
    pub caption_entities: Vec<TextEntity>,
    pub sizes: Vec<PhotoSizeView>,
    pub is_secret: bool,
    pub has_spoiler: bool,
}

impl PhotoContent {
    /// Prefer `photoSize.type == "m"` (box 320), else the largest size ≤ 320px wide.
    pub fn thumb_size(&self) -> Option<&PhotoSizeView> {
        self.sizes
            .iter()
            .find(|size| size.type_name == "m")
            .or_else(|| {
                self.sizes
                    .iter()
                    .filter(|size| size.width > 0 && size.width <= 320)
                    .max_by_key(|size| size.width)
            })
            .or_else(|| {
                self.sizes
                    .iter()
                    .min_by_key(|size| (size.width, size.height))
            })
    }

    pub fn largest_size(&self) -> Option<&PhotoSizeView> {
        self.sizes
            .iter()
            .max_by_key(|size| i64::from(size.width) * i64::from(size.height))
    }

    pub fn open_file_id(&self) -> Option<FileId> {
        self.largest_size()
            .or_else(|| self.thumb_size())
            .map(|size| size.file_id)
    }

    /// Secret photos must not download on placeholder click (schema: show only while tapped).
    pub fn click_requests_download(&self) -> bool {
        !self.is_secret
    }

    /// Placeholder copy follows file state for secret and spoiler photos.
    pub fn placeholder_label(&self, downloading: bool, ready: bool) -> String {
        let kind = if self.is_secret {
            "Secret photo"
        } else if self.has_spoiler {
            "Photo (spoiler)"
        } else {
            "Photo"
        };
        let state = if ready {
            "ready"
        } else if downloading {
            "downloading…"
        } else {
            "not downloaded"
        };
        format!("{kind} — {state}")
    }
}

/// `photoSize` fields used for display / download (schema: type, photo, width, height).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoSizeView {
    pub type_name: String,
    pub width: i32,
    pub height: i32,
    pub file_id: FileId,
}

/// `document` + caption from `messageDocument`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentContent {
    pub file_name: String,
    pub mime_type: String,
    pub caption: String,
    /// Phase 4.1: entities for `caption` (same list as message text).
    pub caption_entities: Vec<TextEntity>,
    pub file_id: FileId,
}

/// Still image vs animation. TGS / WEBM are not played in this slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StickerFormat {
    Webp,
    Tgs,
    Webm,
    Unknown,
}

/// `minithumbnail` (TDLib 1.8.67): JPEG bytes, usually ≤ 40px.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiniThumbnail {
    pub width: i32,
    pub height: i32,
    /// Raw JPEG from `minithumbnail.data` (`bytes`).
    pub data: Vec<u8>,
}

/// One `thumbnail` used as an album cover (`album_cover_thumbnail` or
/// `external_album_covers`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumCoverThumb {
    pub width: i32,
    pub height: i32,
    pub file_id: FileId,
}

/// `audio` inside `messageAudio` (TDLib 1.8.67). Music files, not voice notes.
///
/// Schema: `duration`, `title`, `performer`, `file_name`, `mime_type`,
/// `album_cover_minithumbnail`, `album_cover_thumbnail`,
/// `external_album_covers`, `audio:file`, plus `messageAudio.caption`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioContent {
    pub duration: i32,
    pub title: String,
    pub performer: String,
    pub file_name: String,
    pub mime_type: String,
    pub caption: String,
    /// Phase 4.1: entities for `caption` (same list as message text).
    pub caption_entities: Vec<TextEntity>,
    pub album_cover_minithumbnail: Option<MiniThumbnail>,
    pub album_cover_thumbnail: Option<AlbumCoverThumb>,
    pub external_album_covers: Vec<AlbumCoverThumb>,
    pub file_id: FileId,
}

impl AudioContent {
    /// The track itself (`audio.audio`).
    pub fn play_file_id(&self) -> Option<FileId> {
        (self.file_id.0 != 0).then_some(self.file_id)
    }

    /// Cover to show and auto-download. The sender thumbnail wins; otherwise
    /// the largest `external_album_covers` entry (schema: fallback when the
    /// file has no embedded cover).
    pub fn cover_file_id(&self) -> Option<FileId> {
        if let Some(cover) = &self.album_cover_thumbnail
            && cover.file_id.0 != 0
        {
            return Some(cover.file_id);
        }
        self.external_album_covers
            .iter()
            .filter(|cover| cover.file_id.0 != 0)
            .max_by_key(|cover| i64::from(cover.width) * i64::from(cover.height))
            .map(|cover| cover.file_id)
    }
}

/// `messageVoiceNote` / `voiceNote` (TDLib 1.8.67).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceNoteContent {
    pub duration: i32,
    /// Raw `waveform` bytes (5-bit packed), not the decoded bars.
    pub waveform: Vec<u8>,
    pub mime_type: String,
    pub caption: String,
    /// Phase 4.1: entities for `caption` (same list as message text).
    pub caption_entities: Vec<TextEntity>,
    pub is_listened: bool,
    pub file_id: FileId,
}

/// `messageSticker` (TDLib 1.8.67). Display uses `thumbnail` (WEBP/JPEG) or a WEBP `sticker` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StickerContent {
    pub emoji: String,
    pub width: i32,
    pub height: i32,
    pub format: StickerFormat,
    pub file_id: FileId,
    pub thumb_file_id: Option<FileId>,
    pub thumb_width: i32,
    pub thumb_height: i32,
    pub is_premium: bool,
}

impl StickerContent {
    /// File to show: thumbnail first, else the sticker itself when it is static WEBP.
    pub fn display_file_id(&self) -> Option<FileId> {
        if let Some(id) = self.thumb_file_id.filter(|id| id.0 != 0) {
            return Some(id);
        }
        if self.format == StickerFormat::Webp && self.file_id.0 != 0 {
            return Some(self.file_id);
        }
        None
    }
}

/// One sticker inside `stickerSet.stickers` (picker). Same file ids as `sticker`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StickerItem {
    pub id: i64,
    pub set_id: i64,
    pub emoji: String,
    pub width: i32,
    pub height: i32,
    pub format: StickerFormat,
    pub file_id: FileId,
    pub thumb_file_id: Option<FileId>,
    pub thumb_width: i32,
    pub thumb_height: i32,
}

/// `stickerSetInfo` row from `getInstalledStickerSets` (regular sets only are requested).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StickerSetInfo {
    pub id: i64,
    pub title: String,
    pub name: String,
    pub size: i32,
    pub is_installed: bool,
    pub is_official: bool,
}

/// `animation` inside `messageAnimation` or `getSavedAnimations` (TDLib 1.8.67).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationContent {
    pub duration: i32,
    pub width: i32,
    pub height: i32,
    pub file_name: String,
    pub mime_type: String,
    pub caption: String,
    /// Phase 4.1: entities for `caption` (same list as message text).
    pub caption_entities: Vec<TextEntity>,
    pub show_caption_above_media: bool,
    pub has_spoiler: bool,
    pub is_secret: bool,
    pub file_id: FileId,
    pub thumb_file_id: Option<FileId>,
    pub thumb_width: i32,
    pub thumb_height: i32,
}

impl AnimationContent {
    /// JPEG/MPEG4 thumbnail file, when the sender attached one.
    pub fn thumb_file_id(&self) -> Option<FileId> {
        self.thumb_file_id.filter(|id| id.0 != 0)
    }

    /// The clip itself (`animation.animation`).
    pub fn play_file_id(&self) -> Option<FileId> {
        (self.file_id.0 != 0).then_some(self.file_id)
    }
}

/// `video` inside `messageVideo` (TDLib 1.8.67).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoContent {
    pub duration: i32,
    pub width: i32,
    pub height: i32,
    pub file_name: String,
    pub mime_type: String,
    pub caption: String,
    /// Phase 4.1: entities for `caption` (same list as message text).
    pub caption_entities: Vec<TextEntity>,
    pub show_caption_above_media: bool,
    pub has_spoiler: bool,
    pub is_secret: bool,
    /// `messageVideo.start_timestamp` — seconds to seek before playback.
    pub start_timestamp: i32,
    pub supports_streaming: bool,
    pub has_stickers: bool,
    pub file_id: FileId,
    pub thumb_file_id: Option<FileId>,
    pub thumb_width: i32,
    pub thumb_height: i32,
}

impl VideoContent {
    /// JPEG/MPEG4 thumbnail file, when the sender attached one.
    pub fn thumb_file_id(&self) -> Option<FileId> {
        self.thumb_file_id.filter(|id| id.0 != 0)
    }

    /// The clip itself (`video.video`).
    pub fn play_file_id(&self) -> Option<FileId> {
        (self.file_id.0 != 0).then_some(self.file_id)
    }
}

/// `videoNote` inside `messageVideoNote` (TDLib 1.8.67).
///
/// Schema: square MPEG4 cropped to a circle. Fields stored are `duration`,
/// `waveform`, `length` (width and height), `thumbnail`, `video`, plus
/// `is_viewed` and `is_secret` on the message. `minithumbnail` and
/// `speech_recognition_result` are left unused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoNoteContent {
    pub duration: i32,
    /// Raw `waveform` bytes (5-bit packed). Empty when unknown.
    pub waveform: Vec<u8>,
    /// Video width and height, as defined by the sender.
    pub length: i32,
    pub is_viewed: bool,
    pub is_secret: bool,
    pub file_id: FileId,
    pub thumb_file_id: Option<FileId>,
    pub thumb_width: i32,
    pub thumb_height: i32,
}

impl VideoNoteContent {
    /// JPEG thumbnail file, when the sender attached one.
    pub fn thumb_file_id(&self) -> Option<FileId> {
        self.thumb_file_id.filter(|id| id.0 != 0)
    }

    /// The clip itself (`videoNote.video`). Schema: MPEG4.
    pub fn play_file_id(&self) -> Option<FileId> {
        (self.file_id.0 != 0).then_some(self.file_id)
    }
}

/// One saved GIF from `animations.animations` (picker). Same file ids as `animation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationItem {
    pub duration: i32,
    pub width: i32,
    pub height: i32,
    pub file_name: String,
    pub mime_type: String,
    pub file_id: FileId,
    pub thumb_file_id: Option<FileId>,
    pub thumb_width: i32,
    pub thumb_height: i32,
}

/// Typed `file` + `localFile` (no `remoteFile.id` — that can be an HTTP URL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFile {
    pub id: FileId,
    pub size: i64,
    pub expected_size: i64,
    pub local: LocalFileState,
}

impl ParsedFile {
    pub fn usable_path(&self) -> Option<&str> {
        if self.local.is_downloading_completed && !self.local.path.is_empty() {
            Some(self.local.path.as_str())
        } else {
            None
        }
    }

    pub fn needs_download(&self) -> bool {
        self.usable_path().is_none() && self.local.can_be_downloaded
    }

    pub fn display_size(&self) -> i64 {
        if self.size > 0 {
            self.size
        } else {
            self.expected_size
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalFileState {
    pub path: String,
    pub can_be_downloaded: bool,
    pub is_downloading_active: bool,
    pub is_downloading_completed: bool,
}

impl LocalFileState {
    /// Download is neither in flight nor finished (`file` / `updateFile` idle).
    pub fn is_idle_incomplete(&self) -> bool {
        !self.is_downloading_active && !self.is_downloading_completed
    }
}

#[derive(Debug, Deserialize)]
struct RawEnvelope {
    #[serde(rename = "@type")]
    type_name: String,
    #[serde(rename = "@extra")]
    extra: Option<Value>,
    #[serde(rename = "@client_id")]
    client_id: Option<i32>,
}

pub fn parse_envelope(json: &str) -> Result<Envelope, ParseError> {
    let raw: RawEnvelope = serde_json::from_str(json).map_err(|_| ParseError::InvalidJson)?;
    let extra = parse_extra(raw.extra.as_ref());
    let payload = parse_payload(&raw.type_name, json)?;
    Ok(Envelope {
        type_name: raw.type_name,
        extra,
        client_id: raw.client_id,
        payload,
    })
}

fn parse_extra(value: Option<&Value>) -> Option<RequestId> {
    match value {
        Some(Value::String(s)) => RequestId::from_str(s).ok(),
        Some(Value::Number(n)) => n.as_u64().map(RequestId),
        _ => None,
    }
}

fn parse_payload(type_name: &str, json: &str) -> Result<EnvelopePayload, ParseError> {
    let value: Value = serde_json::from_str(json).map_err(|_| ParseError::InvalidJson)?;
    match type_name {
        "updateAuthorizationState" => {
            let state = value
                .get("authorization_state")
                .ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::UpdateAuthorizationState(parse_auth(state)))
        }
        "authorizationStateWaitTdlibParameters"
        | "authorizationStateWaitPhoneNumber"
        | "authorizationStateWaitPremiumPurchase"
        | "authorizationStateWaitEmailAddress"
        | "authorizationStateWaitEmailCode"
        | "authorizationStateWaitCode"
        | "authorizationStateWaitOtherDeviceConfirmation"
        | "authorizationStateWaitRegistration"
        | "authorizationStateWaitPassword"
        | "authorizationStateReady"
        | "authorizationStateLoggingOut"
        | "authorizationStateClosing"
        | "authorizationStateClosed" => Ok(EnvelopePayload::UpdateAuthorizationState(parse_auth(
            &value,
        ))),
        "updateNewMessage" => {
            let message = value.get("message").ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::UpdateNewMessage(parse_message(message)?))
        }
        "updateMessageSendSucceeded" => Ok(EnvelopePayload::UpdateMessageSendSucceeded {
            message: parse_message(value.get("message").ok_or(ParseError::MissingField)?)?,
            old_message_id: MessageId(int53(value.get("old_message_id"))?),
        }),
        "updateMessageSendFailed" => Ok(EnvelopePayload::UpdateMessageSendFailed {
            message: parse_message(value.get("message").ok_or(ParseError::MissingField)?)?,
            old_message_id: MessageId(int53(value.get("old_message_id"))?),
            error: parse_error(value.get("error")),
        }),
        "updateMessageSendAcknowledged" => Ok(EnvelopePayload::UpdateMessageSendAcknowledged {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            message_id: MessageId(int53(value.get("message_id"))?),
        }),
        "updateDeleteMessages" => Ok(EnvelopePayload::UpdateDeleteMessages {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            message_ids: int53_array(value.get("message_ids")),
            is_permanent: value
                .get("is_permanent")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            from_cache: value
                .get("from_cache")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        "updateMessageContent" => {
            let (content, files) = parse_content(value.get("new_content"));
            Ok(EnvelopePayload::UpdateMessageContent {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                message_id: MessageId(int53(value.get("message_id"))?),
                content,
                files,
            })
        }
        "updateMessageContentOpened" => Ok(EnvelopePayload::UpdateMessageContentOpened {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            message_id: MessageId(int53(value.get("message_id"))?),
        }),
        "updateMessageEdited" => Ok(EnvelopePayload::UpdateMessageEdited {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            message_id: MessageId(int53(value.get("message_id"))?),
            edit_date: value.get("edit_date").and_then(Value::as_i64).unwrap_or(0) as i32,
            reply_markup: parse_reply_markup(value.get("reply_markup")),
        }),
        "updatePoll" => Ok(EnvelopePayload::UpdatePoll {
            poll: parse_poll(value.get("poll")).ok_or(ParseError::MissingField)?,
        }),
        "updateChatPosition" => Ok(EnvelopePayload::UpdateChatPosition(parse_position(&value)?)),
        "updateChatTitle" => Ok(EnvelopePayload::UpdateChatTitle {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            title: value
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        "updateChatLastMessage" => {
            let chat_id = ChatId(int53(value.get("chat_id"))?);
            Ok(EnvelopePayload::UpdateChatLastMessage {
                chat_id,
                last_message: match value.get("last_message") {
                    None | Some(Value::Null) => None,
                    Some(message) => parse_message(message).ok(),
                },
                positions: parse_position_list(chat_id, value.get("positions")),
            })
        }
        "updateChatAddedToList" => Ok(EnvelopePayload::UpdateChatAddedToList {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            list: parse_chat_list(value.get("chat_list")),
        }),
        "updateChatRemovedFromList" => Ok(EnvelopePayload::UpdateChatRemovedFromList {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            list: parse_chat_list(value.get("chat_list")),
        }),
        "updateChatFolders" => {
            let folders = value
                .get("chat_folders")
                .and_then(Value::as_array)
                .map(|arr| arr.iter().filter_map(parse_chat_folder_info).collect())
                .unwrap_or_default();
            let are_tags_enabled = value
                .get("are_tags_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Ok(EnvelopePayload::UpdateChatFolders {
                folders,
                are_tags_enabled,
            })
        }
        "updateChatActiveStories" => {
            let active_stories = value
                .get("active_stories")
                .ok_or(ParseError::MissingField)?;
            parse_chat_active_stories(active_stories)
                .map(|active_stories| EnvelopePayload::UpdateChatActiveStories { active_stories })
                .ok_or(ParseError::MissingField)
        }
        "chatActiveStories" => parse_chat_active_stories(&value)
            .map(|active_stories| EnvelopePayload::ChatActiveStories { active_stories })
            .ok_or(ParseError::MissingField),
        "story" => {
            let (story, files) = parse_story(&value).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::Story { story, files })
        }
        "updateStory" => {
            let story = value.get("story").ok_or(ParseError::MissingField)?;
            let (story, files) = parse_story(story).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::Story { story, files })
        }
        "updateStoryDeleted" => Ok(EnvelopePayload::UpdateStoryDeleted {
            poster_chat_id: int53(value.get("story_poster_chat_id"))?,
            story_id: value
                .get("story_id")
                .and_then(Value::as_i64)
                .ok_or(ParseError::MissingField)? as i32,
        }),
        // Phase B1: secret chat lifecycle (schema 1.8.67, lines 10741 /
        // 2816). `updateSecretChat` carries the full `secretChat` object in
        // its `secret_chat` field; the bare `secretChat` object is the
        // `getSecretChat` answer.
        "updateSecretChat" => Ok(EnvelopePayload::UpdateSecretChat {
            secret_chat: parse_secret_chat(value.get("secret_chat"))
                .ok_or(ParseError::MissingField)?,
        }),
        "secretChat" => Ok(EnvelopePayload::SecretChat {
            secret_chat: parse_secret_chat(Some(&value)).ok_or(ParseError::MissingField)?,
        }),
        // Phase C1: call signaling updates (schema 1.8.67, lines
        // 10816 / 10862) and the `createCall` answer (`callId`,
        // line 7034).
        "updateCall" => Ok(EnvelopePayload::UpdateCall {
            call: parse_call(value.get("call")).ok_or(ParseError::MissingField)?,
        }),
        "updateNewCallSignalingData" => Ok(EnvelopePayload::UpdateNewCallSignalingData {
            call_id: value
                .get("call_id")
                .and_then(Value::as_i64)
                .ok_or(ParseError::MissingField)? as i32,
            data: value
                .get("data")
                .and_then(Value::as_str)
                .and_then(|s| STANDARD.decode(s).ok())
                .unwrap_or_default(),
        }),
        "callId" => Ok(EnvelopePayload::CallId {
            id: value
                .get("id")
                .and_then(Value::as_i64)
                .ok_or(ParseError::MissingField)? as i32,
        }),
        "updateStoryPostSucceeded" => {
            let story = value.get("story").ok_or(ParseError::MissingField)?;
            let (story, files) = parse_story(story).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::UpdateStoryPostSucceeded {
                story,
                files,
                old_story_id: value
                    .get("old_story_id")
                    .and_then(Value::as_i64)
                    .ok_or(ParseError::MissingField)? as i32,
            })
        }
        "updateStoryPostFailed" => {
            let story = value.get("story").ok_or(ParseError::MissingField)?;
            let (story, _files) = parse_story(story).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::UpdateStoryPostFailed {
                story,
                error: parse_error(value.get("error")),
            })
        }
        "availableReactions" => {
            let reactions = value
                .get("top_reactions")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(parse_story_available_reaction)
                        .collect()
                })
                .unwrap_or_default();
            Ok(EnvelopePayload::StoryAvailableReactions { reactions })
        }
        "updateChatReadInbox" => Ok(EnvelopePayload::UpdateChatReadInbox {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            last_read_inbox_message_id: MessageId(int53_or_zero(
                value.get("last_read_inbox_message_id"),
            )),
            unread_count: value
                .get("unread_count")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
        }),
        "updateChatReadOutbox" => Ok(EnvelopePayload::UpdateChatReadOutbox {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            last_read_outbox_message_id: MessageId(int53_or_zero(
                value.get("last_read_outbox_message_id"),
            )),
        }),
        "updateChatNotificationSettings" => Ok(EnvelopePayload::UpdateChatNotificationSettings {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            notification_settings: parse_chat_notification_settings(
                value.get("notification_settings"),
            ),
        }),
        "updateChatAction" => Ok(EnvelopePayload::UpdateChatAction {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            sender: parse_message_sender(value.get("sender_id"))?,
            action: parse_chat_action(value.get("action")),
        }),
        // Parity slice: `updateChatPhoto` (schema 1.8.67, line 10488).
        "updateChatPhoto" => Ok(EnvelopePayload::UpdateChatPhoto {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            photo: parse_chat_photo_small(value.get("photo")),
        }),
        "updateChatDraftMessage" => {
            let chat_id = ChatId(int53(value.get("chat_id"))?);
            Ok(EnvelopePayload::UpdateChatDraftMessage {
                chat_id,
                draft: parse_chat_draft(value.get("draft_message")),
                positions: parse_position_list(chat_id, value.get("positions")),
            })
        }
        "updateUser" => {
            let user = value.get("user").ok_or(ParseError::MissingField)?;
            let parsed = parse_user(user).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::UpdateUser {
                user_id: UserId(parsed.id),
                user: parsed,
            })
        }
        "updateUserStatus" => Ok(EnvelopePayload::UpdateUserStatus {
            user_id: UserId(int53(value.get("user_id"))?),
            status: parse_user_status(value.get("status")),
        }),
        "users" => Ok(EnvelopePayload::Users {
            user_ids: value
                .get("user_ids")
                .and_then(Value::as_array)
                .map(|ids| ids.iter().filter_map(|id| int53(Some(id)).ok()).collect())
                .unwrap_or_default(),
        }),
        "updateConnectionState" => Ok(EnvelopePayload::UpdateConnectionState(parse_connection(
            value.get("state"),
        ))),
        "updateNewChat" => {
            let chat = value.get("chat").ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::UpdateNewChat {
                chat_id: ChatId(int53(chat.get("id"))?),
                title: chat
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                kind: parse_chat_kind(chat.get("type")),
                unread_count: chat
                    .get("unread_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
                last_read_inbox_message_id: MessageId(int53_or_zero(
                    chat.get("last_read_inbox_message_id"),
                )),
                last_read_outbox_message_id: MessageId(int53_or_zero(
                    chat.get("last_read_outbox_message_id"),
                )),
                notification_settings: parse_chat_notification_settings(
                    chat.get("notification_settings"),
                ),
                draft: parse_chat_draft(chat.get("draft_message")),
                // Parity slice: `chat.photo.small` (`chatPhotoInfo`, schema
                // 1.8.67, lines 762 and 3627).
                photo: parse_chat_photo_small(chat.get("photo")),
                // Parity slice 4: `chat.permissions.can_send_basic_messages`
                // (schema 1.8.67, line 1070). Lenient default true — the
                // real `chat` object always carries `permissions`.
                can_send_basic_messages: chat
                    .get("permissions")
                    .and_then(|p| p.get("can_send_basic_messages"))
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            })
        }
        "updateChatPermissions" => {
            // Parity slice 4: `updateChatPermissions` (schema 1.8.67, line
            // 10500) — keep the send-permission gate fresh.
            let permissions = value.get("permissions");
            Ok(EnvelopePayload::UpdateChatPermissions {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                can_send_basic_messages: permissions
                    .and_then(|p| p.get("can_send_basic_messages"))
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            })
        }
        "ok" => Ok(EnvelopePayload::Ok),
        "callbackQueryAnswer" => Ok(EnvelopePayload::CallbackQueryAnswer(
            parse_callback_query_answer(&value),
        )),
        // Parity slice: `createChatFolder` / `editChatFolder` responses
        // (TDLib 1.8.67, `schema/td_api.tl:13358` / `:13361`).
        "chatFolderInfo" => parse_chat_folder_info(&value)
            .map(EnvelopePayload::ChatFolderInfo)
            .ok_or(ParseError::MissingField),
        // Parity slice: `getChatFolder` response (TDLib 1.8.67,
        // `schema/td_api.tl:13355`) — the full editable folder spec.
        "chatFolder" => parse_chat_folder(&value)
            .map(|spec| EnvelopePayload::ChatFolder { spec })
            .ok_or(ParseError::MissingField),
        // Parity slice: `getChatListsToAddChat` response (TDLib 1.8.67,
        // `schema/td_api.tl:13347`) — the chat lists a chat may be added
        // to via `addChatToList`.
        "chatLists" => {
            let lists = value
                .get("chat_lists")
                .and_then(Value::as_array)
                .map(|arr| arr.iter().map(|v| parse_chat_list(Some(v))).collect())
                .unwrap_or_default();
            Ok(EnvelopePayload::ChatLists { lists })
        }
        "error" => Ok(EnvelopePayload::Error(parse_error(Some(&value)))),
        "messages" => {
            let messages = value
                .get("messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let parsed = messages
                .iter()
                .filter_map(|m| parse_message(m).ok())
                .collect();
            Ok(EnvelopePayload::Messages(parsed))
        }
        "chats" => Ok(EnvelopePayload::Chats {
            total_count: value
                .get("total_count")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            chat_ids: value
                .get("chat_ids")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|v| int53(Some(v)).ok())
                .map(ChatId)
                .collect(),
        }),
        "foundMessages" => {
            let messages = value
                .get("messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let parsed = messages
                .iter()
                .filter_map(|m| parse_message(m).ok())
                .collect();
            Ok(EnvelopePayload::FoundMessages {
                total_count: value
                    .get("total_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
                messages: parsed,
                next_offset: value
                    .get("next_offset")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
        }
        "foundChatMessages" => {
            let messages = value
                .get("messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let parsed = messages
                .iter()
                .filter_map(|m| parse_message(m).ok())
                .collect();
            Ok(EnvelopePayload::FoundChatMessages {
                total_count: value
                    .get("total_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
                messages: parsed,
                next_from_message_id: MessageId(int53_or_zero(value.get("next_from_message_id"))),
            })
        }
        // Phase 5.1: `updateSupergroup` (schema line 10738) and the
        // `getSupergroup` response both carry `supergroup.is_forum` (schema
        // line 2746). Parity slice: also keep the first active username
        // (`supergroup.usernames`, schema lines 2746/2372) for the
        // channel/supergroup header.
        "updateSupergroup" => {
            let supergroup = value.get("supergroup").ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::UpdateSupergroup {
                supergroup_id: int53(supergroup.get("id"))?,
                is_forum: supergroup
                    .get("is_forum")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                username: parse_first_active_username(supergroup.get("usernames")),
                // Phase A1: own `chatMemberStatus*` (schema 1.8.67 line
                // 2746); unknown/missing → `Unknown` (gated, no bypass).
                // `can_restrict_members` gates the slow-mode admin control
                // (schema line 13551).
                status: parse_channel_member_status(supergroup.get("status"))
                    .map(|(status, _)| status)
                    .unwrap_or(ChannelMemberStatus::Unknown),
                can_restrict_members: parse_restrict_members_right(supergroup.get("status")),
            })
        }
        "supergroup" => Ok(EnvelopePayload::Supergroup {
            supergroup_id: int53(value.get("id"))?,
            is_forum: value
                .get("is_forum")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            username: parse_first_active_username(value.get("usernames")),
            // Phase A1: own `chatMemberStatus*` (schema 1.8.67 line 2746).
            status: parse_channel_member_status(value.get("status"))
                .map(|(status, _)| status)
                .unwrap_or(ChannelMemberStatus::Unknown),
            can_restrict_members: parse_restrict_members_right(value.get("status")),
        }),
        // Phase 5.1: `forumTopics` (schema line 3976). Topics keep their
        // response order; the UI sorts by `order` descending per the schema
        // ("Topics must be sorted by the order in descending order").
        "forumTopics" => {
            let topics = value
                .get("topics")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let parsed = topics.iter().filter_map(parse_forum_topic).collect();
            Ok(EnvelopePayload::ForumTopics {
                total_count: value
                    .get("total_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
                topics: parsed,
            })
        }
        "message" => Ok(EnvelopePayload::Message(parse_message(&value)?)),
        "updateFile" => Ok(EnvelopePayload::UpdateFile(parse_file(value.get("file"))?)),
        "file" => Ok(EnvelopePayload::File(parse_file(Some(&value))?)),
        "stickerSets" => Ok(parse_sticker_sets(&value)),
        "stickerSet" => Ok(parse_sticker_set(&value)),
        "animations" => Ok(parse_animations(&value)),
        "sponsoredMessages" => Ok(parse_sponsored_messages(&value)?),
        "reportSponsoredResultOk" => Ok(EnvelopePayload::ReportSponsoredResult(
            ReportSponsoredResult::Ok,
        )),
        "reportSponsoredResultFailed" => Ok(EnvelopePayload::ReportSponsoredResult(
            ReportSponsoredResult::Failed,
        )),
        "reportSponsoredResultOptionRequired" => Ok(EnvelopePayload::ReportSponsoredResult(
            ReportSponsoredResult::OptionRequired {
                title: json_field_str(&value, "title"),
                options: parse_report_options(value.get("options")),
            },
        )),
        "reportSponsoredResultAdsHidden" => Ok(EnvelopePayload::ReportSponsoredResult(
            ReportSponsoredResult::AdsHidden,
        )),
        "reportSponsoredResultPremiumRequired" => Ok(EnvelopePayload::ReportSponsoredResult(
            ReportSponsoredResult::PremiumRequired,
        )),
        "updateSavedAnimations" => Ok(EnvelopePayload::UpdateSavedAnimations {
            animation_ids: value
                .get("animation_ids")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| id.as_i64())
                        .map(|id| id as i32)
                        .filter(|id| *id != 0)
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "notificationSounds" => {
            let sounds = value
                .get("notification_sounds")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(parse_notification_sound).collect())
                .unwrap_or_default();
            Ok(EnvelopePayload::NotificationSounds { sounds })
        }
        "updateSavedNotificationSounds" => Ok(EnvelopePayload::UpdateSavedNotificationSounds {
            sound_ids: value
                .get("notification_sound_ids")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| id.as_i64().or_else(|| id.as_str()?.parse().ok()))
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "scopeNotificationSettings" => {
            // The response to `getScopeNotificationSettings` carries no scope
            // field — the scope is correlated via the pending request.
            Ok(EnvelopePayload::ScopeNotificationSettings {
                scope: NotificationSettingsScope::PrivateChats,
                settings: parse_scope_notification_settings(Some(&value)),
            })
        }
        "updateScopeNotificationSettings" => {
            let scope = value
                .get("scope")
                .and_then(|s| s.get("@type"))
                .and_then(Value::as_str);
            match parse_notification_settings_scope(scope) {
                Some(scope) => Ok(EnvelopePayload::UpdateScopeNotificationSettings {
                    scope,
                    settings: parse_scope_notification_settings(value.get("notification_settings")),
                }),
                None => Err(ParseError::MissingField),
            }
        }
        "updateMessageInteractionInfo" => Ok(EnvelopePayload::UpdateMessageInteractionInfo {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            message_id: MessageId(int53(value.get("message_id"))?),
            interaction_info: parse_interaction_info(value.get("interaction_info")),
        }),
        "updateChatMember" => {
            let member = value
                .get("new_chat_member")
                .and_then(|m| parse_chat_member(Some(m)))
                .ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::UpdateChatMember {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                member,
            })
        }
        "chatMember" => {
            let member = parse_chat_member(Some(&value)).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::ChatMember { member })
        }
        "user" => Ok(EnvelopePayload::Me {
            user_id: int53(value.get("id"))?,
        }),
        "userFullInfo" => Ok(EnvelopePayload::UserFullInfo {
            bot_info: parse_bot_info(value.get("bot_info")),
            bio: parse_formatted_text(value.get("bio")),
            photo: parse_user_full_info_photo(&value),
        }),
        "updateUserFullInfo" => Ok(EnvelopePayload::UpdateUserFullInfo {
            user_id: UserId(int53(value.get("user_id"))?),
            bot_info: parse_bot_info(
                value
                    .get("user_full_info")
                    .and_then(|info| info.get("bot_info")),
            ),
            bio: parse_formatted_text(value.get("user_full_info").and_then(|info| info.get("bio"))),
            photo: value
                .get("user_full_info")
                .and_then(parse_user_full_info_photo),
        }),
        "supergroupFullInfo" => Ok(EnvelopePayload::SupergroupFullInfo {
            description: value
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            member_count: value
                .get("member_count")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            // Parity slice: `linked_chat_id` (schema 1.8.67, line 2792) —
            // the discussion-group chat id (0 = none).
            linked_chat_id: int53_or_zero(value.get("linked_chat_id")),
            // Phase A1: slow-mode fields (schema 1.8.67, lines 2758–2759)
            // plus the boost bypass counts (lines 2779–2780). The expiry is
            // `double` in the schema; `as_f64` accepts integer JSON too.
            slow_mode_delay: value
                .get("slow_mode_delay")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            slow_mode_delay_expires_in: value
                .get("slow_mode_delay_expires_in")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            my_boost_count: value
                .get("my_boost_count")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            unrestrict_boost_count: value
                .get("unrestrict_boost_count")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
        }),
        // Parity slice: `updateSupergroupFullInfo` (schema 1.8.67, line
        // 10750) — same fields as the `supergroupFullInfo` response, with
        // an explicit `supergroup_id` so no pending-request correlation
        // is needed.
        "updateSupergroupFullInfo" => Ok(EnvelopePayload::UpdateSupergroupFullInfo {
            supergroup_id: int53(value.get("supergroup_id"))?,
            description: value
                .get("supergroup_full_info")
                .and_then(|info| info.get("description"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            member_count: value
                .get("supergroup_full_info")
                .and_then(|info| info.get("member_count"))
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            linked_chat_id: int53_or_zero(
                value
                    .get("supergroup_full_info")
                    .and_then(|info| info.get("linked_chat_id")),
            ),
            // Phase A1: slow-mode + boost fields (schema 1.8.67,
            // lines 2758–2759 / 2779–2780), nested like the other fields.
            slow_mode_delay: value
                .get("supergroup_full_info")
                .and_then(|info| info.get("slow_mode_delay"))
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            slow_mode_delay_expires_in: value
                .get("supergroup_full_info")
                .and_then(|info| info.get("slow_mode_delay_expires_in"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            my_boost_count: value
                .get("supergroup_full_info")
                .and_then(|info| info.get("my_boost_count"))
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            unrestrict_boost_count: value
                .get("supergroup_full_info")
                .and_then(|info| info.get("unrestrict_boost_count"))
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
        }),
        "botCommands" => Ok(EnvelopePayload::BotCommands {
            bot_user_id: UserId(int53(value.get("bot_user_id"))?),
            commands: value
                .get("commands")
                .and_then(Value::as_array)
                .map(|commands| {
                    commands
                        .iter()
                        .filter_map(|command| parse_bot_command(Some(command)))
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "chatJoinResultSuccess" => Ok(EnvelopePayload::JoinChatResult(ChatJoinResult::Success {
            chat_id: ChatId(int53(value.get("chat_id"))?),
        })),
        "chatJoinResultRequestSent" => {
            Ok(EnvelopePayload::JoinChatResult(ChatJoinResult::RequestSent))
        }
        "chatJoinResultGuardBotApprovalRequired" => Ok(EnvelopePayload::JoinChatResult(
            ChatJoinResult::GuardBotApprovalRequired,
        )),
        "chatJoinResultDeclined" => Ok(EnvelopePayload::JoinChatResult(ChatJoinResult::Declined)),
        "updateMessageIsPinned" => Ok(EnvelopePayload::UpdateMessageIsPinned {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            message_id: MessageId(int53(value.get("message_id"))?),
            is_pinned: value
                .get("is_pinned")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        other => Ok(EnvelopePayload::Unknown(UnknownKind {
            type_name: other.to_string(),
        })),
    }
}

fn parse_auth(value: &Value) -> AuthorizationState {
    let ty = value.get("@type").and_then(Value::as_str).unwrap_or("");
    match ty {
        "authorizationStateWaitTdlibParameters" => AuthorizationState::WaitTdlibParameters,
        "authorizationStateWaitPhoneNumber" => AuthorizationState::WaitPhoneNumber,
        "authorizationStateWaitPremiumPurchase" => AuthorizationState::WaitPremiumPurchase,
        "authorizationStateWaitEmailAddress" => AuthorizationState::WaitEmailAddress,
        "authorizationStateWaitEmailCode" => AuthorizationState::WaitEmailCode,
        "authorizationStateWaitCode" => AuthorizationState::WaitCode {
            code_length: value
                .get("code_info")
                .and_then(|info| info.get("type"))
                .and_then(|ty| ty.get("length"))
                .and_then(Value::as_i64)
                .map(|n| n as i32),
        },
        "authorizationStateWaitOtherDeviceConfirmation" => {
            AuthorizationState::WaitOtherDeviceConfirmation
        }
        "authorizationStateWaitRegistration" => AuthorizationState::WaitRegistration,
        "authorizationStateWaitPassword" => AuthorizationState::WaitPassword {
            has_recovery_email: value
                .get("has_recovery_email_address")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        "authorizationStateReady" => AuthorizationState::Ready,
        "authorizationStateLoggingOut" => AuthorizationState::LoggingOut,
        "authorizationStateClosing" => AuthorizationState::Closing,
        "authorizationStateClosed" => AuthorizationState::Closed,
        other => AuthorizationState::Unknown(other.to_string()),
    }
}

fn parse_connection(value: Option<&Value>) -> ConnectionState {
    match value.and_then(|v| v.get("@type")).and_then(Value::as_str) {
        Some("connectionStateWaitingForNetwork") => ConnectionState::WaitingForNetwork,
        Some("connectionStateConnectingToProxy") => ConnectionState::ConnectingToProxy,
        Some("connectionStateConnecting") => ConnectionState::Connecting,
        Some("connectionStateUpdating") => ConnectionState::Updating,
        Some("connectionStateReady") => ConnectionState::Ready,
        _ => ConnectionState::Unknown,
    }
}

fn parse_chat_kind(value: Option<&Value>) -> ChatKind {
    let Some(value) = value else {
        return ChatKind::Unknown;
    };
    match value.get("@type").and_then(Value::as_str) {
        Some("chatTypePrivate") => ChatKind::Private {
            user_id: UserId(int53(value.get("user_id")).unwrap_or(0)),
        },
        Some("chatTypeBasicGroup") => ChatKind::BasicGroup {
            basic_group_id: int53(value.get("basic_group_id")).unwrap_or(0),
        },
        Some("chatTypeSupergroup") => ChatKind::Supergroup {
            supergroup_id: int53(value.get("supergroup_id")).unwrap_or(0),
            is_channel: value
                .get("is_channel")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        Some("chatTypeSecret") => ChatKind::Secret {
            secret_chat_id: value
                .get("secret_chat_id")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            user_id: UserId(int53(value.get("user_id")).unwrap_or(0)),
        },
        _ => ChatKind::Unknown,
    }
}

fn parse_position(value: &Value) -> Result<ChatPositionUpdate, ParseError> {
    let chat_id = ChatId(int53(value.get("chat_id"))?);
    let position = value.get("position").unwrap_or(value);
    Ok(parse_position_entry(chat_id, position))
}

fn parse_position_entry(chat_id: ChatId, position: &Value) -> ChatPositionUpdate {
    ChatPositionUpdate {
        chat_id,
        list: parse_chat_list(position.get("list")),
        order: int64(position.get("order")).unwrap_or(0),
        is_pinned: position
            .get("is_pinned")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

fn parse_position_list(chat_id: ChatId, value: Option<&Value>) -> Vec<ChatPositionUpdate> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|position| parse_position_entry(chat_id, position))
        .collect()
}

fn json_bool(value: Option<&Value>, default: bool) -> bool {
    value.and_then(Value::as_bool).unwrap_or(default)
}

fn json_i32(value: Option<&Value>, default: i32) -> i32 {
    value
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(default as i64) as i32
}

fn json_i64_field(value: Option<&Value>, default: i64) -> i64 {
    value
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(default)
}

fn parse_message_sender(value: Option<&Value>) -> Result<MessageSender, ParseError> {
    let value = value.ok_or(ParseError::MissingField)?;
    match value.get("@type").and_then(Value::as_str) {
        Some("messageSenderUser") => Ok(MessageSender::User {
            user_id: int53(value.get("user_id"))?,
        }),
        Some("messageSenderChat") => Ok(MessageSender::Chat {
            chat_id: int53(value.get("chat_id"))?,
        }),
        _ => Err(ParseError::MissingField),
    }
}

/// `chatMemberStatus*` (TDLib 1.8.67). Unknown constructors map to `Unknown`;
/// the field itself stays required. Also returns `rights.can_post_messages`
/// (`Some`) when the status is `chatMemberStatusAdministrator` and the
/// `rights` block parses; `None` otherwise.
fn parse_channel_member_status(
    value: Option<&Value>,
) -> Option<(ChannelMemberStatus, Option<bool>)> {
    let value = value?;
    let status = match value.get("@type").and_then(Value::as_str) {
        Some("chatMemberStatusCreator") => ChannelMemberStatus::Creator,
        Some("chatMemberStatusAdministrator") => ChannelMemberStatus::Administrator,
        Some("chatMemberStatusMember") => ChannelMemberStatus::Member,
        Some("chatMemberStatusRestricted") => ChannelMemberStatus::Restricted,
        Some("chatMemberStatusLeft") => ChannelMemberStatus::Left,
        Some("chatMemberStatusBanned") => ChannelMemberStatus::Banned,
        _ => ChannelMemberStatus::Unknown,
    };
    let admin_can_post_messages = if status == ChannelMemberStatus::Administrator {
        value
            .get("rights")
            .and_then(|rights| rights.get("can_post_messages"))
            .and_then(Value::as_bool)
    } else {
        None
    };
    Some((status, admin_can_post_messages))
}

/// `rights.can_restrict_members` from a `chatMemberStatusAdministrator`
/// block (TDLib 1.8.67, lines 2500/1092); `None` for any other status or a
/// missing/absent rights block. `setChatSlowModeDelay` requires this
/// right (schema line 13551).
fn parse_restrict_members_right(value: Option<&Value>) -> Option<bool> {
    let value = value?;
    if value.get("@type").and_then(Value::as_str) != Some("chatMemberStatusAdministrator") {
        return None;
    }
    value
        .get("rights")
        .and_then(|rights| rights.get("can_restrict_members"))
        .and_then(Value::as_bool)
}

/// `chatMember` (TDLib 1.8.67). Returns `None` when `member_id` or `status`
/// is missing or unparseable.
fn parse_chat_member(value: Option<&Value>) -> Option<ParsedChatMember> {
    let value = value.filter(|v| !v.is_null())?;
    let member_id = parse_message_sender(value.get("member_id")).ok()?;
    let (status, admin_can_post_messages) = parse_channel_member_status(value.get("status"))?;
    Some(ParsedChatMember {
        member_id,
        status,
        admin_can_post_messages,
    })
}

/// `botCommand` (schema 1.8.67 line 826). A command without `command` text
/// is dropped; the description may be empty.
fn parse_bot_command(value: Option<&Value>) -> Option<BotCommand> {
    let value = value.filter(|v| !v.is_null())?;
    Some(BotCommand {
        command: value.get("command")?.as_str()?.to_string(),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

/// `botInfo` subset (schema 1.8.67 line 2430). `bot_info` arrives as null
/// for non-bots (the schema comment says "may be null if the user isn't a
/// bot"), so null → `None` rather than an empty struct.
fn parse_bot_info(value: Option<&Value>) -> Option<BotInfo> {
    let value = value.filter(|v| !v.is_null())?;
    Some(BotInfo {
        short_description: value
            .get("short_description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        commands: value
            .get("commands")
            .and_then(Value::as_array)
            .map(|commands| {
                commands
                    .iter()
                    .filter_map(|v| parse_bot_command(Some(v)))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// Phase 6: `userStatus*` parser — null/absent/unknown → `Empty`.
fn parse_user_status(value: Option<&Value>) -> UserStatusKind {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return UserStatusKind::Empty;
    };
    match value.get("@type").and_then(Value::as_str) {
        Some("userStatusOnline") => UserStatusKind::Online,
        Some("userStatusOffline") => UserStatusKind::Offline {
            was_online: value.get("was_online").and_then(Value::as_i64).unwrap_or(0) as i32,
        },
        Some("userStatusRecently") => UserStatusKind::Recently,
        Some("userStatusLastWeek") => UserStatusKind::LastWeek,
        Some("userStatusLastMonth") => UserStatusKind::LastMonth,
        _ => UserStatusKind::Empty,
    }
}

/// Parity slice: first entry of `usernames.active_usernames` (schema
/// 1.8.67, line 2372 — "the first one must be shown as the primary
/// username"). Null/absent/empty → empty string.
fn parse_first_active_username(value: Option<&Value>) -> String {
    value
        .filter(|v| !v.is_null())
        .and_then(|u| u.get("active_usernames"))
        .and_then(Value::as_array)
        .and_then(|names| names.first())
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Phase 6: full `user` parser (schema 1.8.67, line 2403). `None` when the
/// object carries no id.
fn parse_user(value: &Value) -> Option<ParsedUser> {
    let id = int53(value.get("id")).ok()?;
    let first_name = value
        .get("first_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let last_name = value
        .get("last_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let username = parse_first_active_username(value.get("usernames"));
    let phone_number = value
        .get("phone_number")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let is_contact = value
        .get("is_contact")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_bot = value
        .get("type")
        .and_then(|t| t.get("@type"))
        .and_then(Value::as_str)
        == Some("userTypeBot");
    let status = parse_user_status(value.get("status"));
    let photo_small_file_id = i32::try_from(int53_or_zero(
        value
            .get("profile_photo")
            .and_then(|p| p.get("small"))
            .and_then(|f| f.get("id")),
    ))
    .unwrap_or(0);
    Some(ParsedUser {
        id,
        first_name,
        last_name,
        username,
        phone_number,
        is_contact,
        is_bot,
        status,
        photo_small_file_id,
    })
}

/// `replyMarkupInlineKeyboard` (TDLib 1.8.67 line 3855). `reply_markup`
/// null/absent and other markup constructors → `None`. Rows and buttons are
/// parsed tolerantly: malformed rows are skipped, malformed buttons become
/// disabled `Unknown` placeholders — a hostile keyboard can never crash the
/// parse.
fn parse_reply_markup(value: Option<&Value>) -> Option<InlineKeyboard> {
    let value = value.filter(|v| !v.is_null())?;
    if value.get("@type").and_then(Value::as_str) != Some("replyMarkupInlineKeyboard") {
        return None;
    }
    let rows = value
        .get("rows")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(Value::as_array)
                .map(|row| {
                    row.iter()
                        .map(parse_inline_keyboard_button)
                        .collect::<Vec<_>>()
                })
                .collect()
        })
        .unwrap_or_default();
    Some(InlineKeyboard {
        rows,
        force_reply: value
            .get("force_reply")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn parse_inline_keyboard_button(value: &Value) -> InlineKeyboardButton {
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let style = match value
        .get("style")
        .and_then(|style| style.get("@type"))
        .and_then(Value::as_str)
    {
        Some("buttonStylePrimary") => InlineKeyboardButtonStyle::Primary,
        Some("buttonStyleDanger") => InlineKeyboardButtonStyle::Danger,
        Some("buttonStyleSuccess") => InlineKeyboardButtonStyle::Success,
        Some("buttonStyleLink") => InlineKeyboardButtonStyle::Link,
        _ => InlineKeyboardButtonStyle::Default,
    };
    InlineKeyboardButton {
        text,
        style,
        kind: parse_inline_keyboard_button_type(value.get("type")),
    }
}

fn parse_inline_keyboard_button_type(value: Option<&Value>) -> InlineKeyboardButtonType {
    let value = value.filter(|v| !v.is_null());
    let Some(value) = value else {
        return InlineKeyboardButtonType::Unknown {
            type_name: String::new(),
        };
    };
    let type_name = value
        .get("@type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    match type_name.as_str() {
        "inlineKeyboardButtonTypeUrl" => InlineKeyboardButtonType::Url {
            url: json_field_str(value, "url"),
        },
        "inlineKeyboardButtonTypeLoginUrl" => InlineKeyboardButtonType::LoginUrl {
            url: json_field_str(value, "url"),
        },
        "inlineKeyboardButtonTypeWebApp" => InlineKeyboardButtonType::WebApp {
            url: json_field_str(value, "url"),
        },
        "inlineKeyboardButtonTypeCallback" => InlineKeyboardButtonType::Callback {
            data: parse_tdlib_bytes(value.get("data")),
        },
        "inlineKeyboardButtonTypeCallbackWithPassword" => {
            InlineKeyboardButtonType::CallbackWithPassword {
                data: parse_tdlib_bytes(value.get("data")),
            }
        }
        "inlineKeyboardButtonTypeCallbackGame" => InlineKeyboardButtonType::CallbackGame,
        "inlineKeyboardButtonTypeSwitchInline" => InlineKeyboardButtonType::SwitchInline {
            query: json_field_str(value, "query"),
            target: parse_inline_keyboard_target_chat(value.get("target_chat")),
        },
        "inlineKeyboardButtonTypeBuy" => InlineKeyboardButtonType::Buy,
        "inlineKeyboardButtonTypeUser" => InlineKeyboardButtonType::User {
            user_id: value.get("user_id").and_then(Value::as_i64).unwrap_or(0),
        },
        "inlineKeyboardButtonTypeCopyText" => InlineKeyboardButtonType::CopyText {
            text: json_field_str(value, "text"),
        },
        "inlineKeyboardButtonTypeDisabled" => InlineKeyboardButtonType::Disabled,
        _ => InlineKeyboardButtonType::Unknown { type_name },
    }
}

fn parse_inline_keyboard_target_chat(value: Option<&Value>) -> InlineKeyboardTargetChat {
    match value
        .filter(|v| !v.is_null())
        .and_then(|v| v.get("@type"))
        .and_then(Value::as_str)
    {
        Some("targetChatCurrent") => InlineKeyboardTargetChat::Current,
        Some("targetChatChosen") => InlineKeyboardTargetChat::Chosen,
        Some("targetChatInternalLink") => InlineKeyboardTargetChat::InternalLink,
        _ => InlineKeyboardTargetChat::Unknown,
    }
}

/// `callbackQueryAnswer` (TDLib 1.8.67 line 7747).
fn parse_callback_query_answer(value: &Value) -> CallbackQueryAnswer {
    CallbackQueryAnswer {
        text: json_field_str(value, "text"),
        show_alert: value
            .get("show_alert")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        url: json_field_str(value, "url"),
    }
}

/// `draftMessage` / `draftMessageContentText`. Other content constructors are
/// not restored into the text field (Unigram only fills the field from text).
fn parse_chat_draft(value: Option<&Value>) -> Option<ChatDraft> {
    let value = value.filter(|v| !v.is_null())?;
    if value.get("@type").and_then(Value::as_str) != Some("draftMessage") {
        return None;
    }
    let content = value.get("content")?;
    if content.get("@type").and_then(Value::as_str) != Some("draftMessageContentText") {
        return None;
    }
    let text = content
        .get("text")
        .and_then(|formatted| formatted.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let reply_to_message_id = value.get("reply_to").and_then(|reply| {
        if reply.get("@type").and_then(Value::as_str) != Some("inputMessageReplyToMessage") {
            return None;
        }
        int53(reply.get("message_id")).ok().map(MessageId)
    });
    if text.trim().is_empty() && reply_to_message_id.is_none() {
        return None;
    }
    Some(ChatDraft {
        text,
        reply_to_message_id,
    })
}

fn parse_chat_action(value: Option<&Value>) -> ChatAction {
    match value.and_then(|v| v.get("@type")).and_then(Value::as_str) {
        Some("chatActionTyping") => ChatAction::Typing,
        None | Some("chatActionCancel") => ChatAction::Cancel,
        Some(_) => ChatAction::Other,
    }
}

fn parse_chat_notification_settings(value: Option<&Value>) -> ChatNotificationSettings {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return ChatNotificationSettings::default();
    };
    let defaults = ChatNotificationSettings::default();
    ChatNotificationSettings {
        use_default_mute_for: json_bool(
            value.get("use_default_mute_for"),
            defaults.use_default_mute_for,
        ),
        mute_for: json_i32(value.get("mute_for"), defaults.mute_for),
        use_default_sound: json_bool(value.get("use_default_sound"), defaults.use_default_sound),
        sound_id: json_i64_field(value.get("sound_id"), defaults.sound_id),
        use_default_show_preview: json_bool(
            value.get("use_default_show_preview"),
            defaults.use_default_show_preview,
        ),
        show_preview: json_bool(value.get("show_preview"), defaults.show_preview),
        use_default_mute_stories: json_bool(
            value.get("use_default_mute_stories"),
            defaults.use_default_mute_stories,
        ),
        mute_stories: json_bool(value.get("mute_stories"), defaults.mute_stories),
        use_default_story_sound: json_bool(
            value.get("use_default_story_sound"),
            defaults.use_default_story_sound,
        ),
        story_sound_id: json_i64_field(value.get("story_sound_id"), defaults.story_sound_id),
        use_default_show_story_poster: json_bool(
            value.get("use_default_show_story_poster"),
            defaults.use_default_show_story_poster,
        ),
        show_story_poster: json_bool(value.get("show_story_poster"), defaults.show_story_poster),
        use_default_disable_pinned_message_notifications: json_bool(
            value.get("use_default_disable_pinned_message_notifications"),
            defaults.use_default_disable_pinned_message_notifications,
        ),
        disable_pinned_message_notifications: json_bool(
            value.get("disable_pinned_message_notifications"),
            defaults.disable_pinned_message_notifications,
        ),
        use_default_disable_mention_notifications: json_bool(
            value.get("use_default_disable_mention_notifications"),
            defaults.use_default_disable_mention_notifications,
        ),
        disable_mention_notifications: json_bool(
            value.get("disable_mention_notifications"),
            defaults.disable_mention_notifications,
        ),
    }
}

/// `notificationSound` (TDLib 1.8.67, line 8857). Returns `None` when the
/// object is missing or its `sound` file does not parse.
fn parse_notification_sound(value: &Value) -> Option<NotificationSound> {
    if value.get("@type").and_then(Value::as_str) != Some("notificationSound") {
        return None;
    }
    let sound = parse_file(value.get("sound")).ok()?;
    Some(NotificationSound {
        id: json_i64_field(value.get("id"), 0),
        duration: json_i32(value.get("duration"), 0),
        date: json_i32(value.get("date"), 0),
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        data: value
            .get("data")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        sound,
    })
}

/// `NotificationSettingsScope` from its constructor name (td_api.tl lines
/// 3337–3343). Unknown names map to `None` so a future scope never
/// mis-files settings.
fn parse_notification_settings_scope(type_name: Option<&str>) -> Option<NotificationSettingsScope> {
    match type_name {
        Some("notificationSettingsScopePrivateChats") => {
            Some(NotificationSettingsScope::PrivateChats)
        }
        Some("notificationSettingsScopeGroupChats") => Some(NotificationSettingsScope::GroupChats),
        Some("notificationSettingsScopeChannelChats") => {
            Some(NotificationSettingsScope::ChannelChats)
        }
        _ => None,
    }
}

/// `scopeNotificationSettings` (TDLib 1.8.67, line 3375).
fn parse_scope_notification_settings(value: Option<&Value>) -> ScopeNotificationSettings {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return ScopeNotificationSettings::default();
    };
    let defaults = ScopeNotificationSettings::default();
    ScopeNotificationSettings {
        mute_for: json_i32(value.get("mute_for"), defaults.mute_for),
        sound_id: json_i64_field(value.get("sound_id"), defaults.sound_id),
        show_preview: json_bool(value.get("show_preview"), defaults.show_preview),
        use_default_mute_stories: json_bool(
            value.get("use_default_mute_stories"),
            defaults.use_default_mute_stories,
        ),
        mute_stories: json_bool(value.get("mute_stories"), defaults.mute_stories),
        story_sound_id: json_i64_field(value.get("story_sound_id"), defaults.story_sound_id),
        show_story_poster: json_bool(value.get("show_story_poster"), defaults.show_story_poster),
        disable_pinned_message_notifications: json_bool(
            value.get("disable_pinned_message_notifications"),
            defaults.disable_pinned_message_notifications,
        ),
        disable_mention_notifications: json_bool(
            value.get("disable_mention_notifications"),
            defaults.disable_mention_notifications,
        ),
    }
}

fn parse_chat_list(value: Option<&Value>) -> ChatList {
    match value.and_then(|v| v.get("@type")).and_then(Value::as_str) {
        Some("chatListMain") => ChatList::Main,
        Some("chatListArchive") => ChatList::Archive,
        Some("chatListFolder") => ChatList::Folder(
            value
                .and_then(|v| v.get("chat_folder_id"))
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
        ),
        _ => ChatList::Unknown,
    }
}

/// `chatFolderInfo` (TDLib 1.8.67, `schema/td_api.tl:3485`): id from
/// `chat_folder_id`-style int32, name from `chatFolderName` (`:3458`) →
/// `formattedText.text` (`:117`), icon from `chatFolderIcon.name` (`:3453`).
fn parse_chat_folder_info(value: &Value) -> Option<ChatFolderInfo> {
    if value.get("@type").and_then(Value::as_str) != Some("chatFolderInfo") {
        return None;
    }
    let name = value
        .get("name")
        .and_then(|v| v.get("text"))
        .and_then(|v| v.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let icon_name = value
        .get("icon")
        .and_then(|v| v.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Some(ChatFolderInfo {
        id: value.get("id").and_then(Value::as_i64).unwrap_or(0) as i32,
        name,
        icon_name,
        color_id: value.get("color_id").and_then(Value::as_i64).unwrap_or(-1) as i32,
    })
}

/// Parity slice: parse the full `chatFolder` spec (TDLib 1.8.67,
/// `schema/td_api.tl:3476`) from a `getChatFolder` response (`:13355`).
/// `name` carries only CustomEmoji entities per the schema docs, so the
/// plain text is taken. Returns `None` when `@type` is not `chatFolder`.
fn parse_chat_folder(value: &Value) -> Option<ChatFolderSpec> {
    if value.get("@type").and_then(Value::as_str) != Some("chatFolder") {
        return None;
    }
    let name = value
        .get("name")
        .and_then(|v| v.get("text"))
        .and_then(|v| v.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let ids = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(Value::as_i64).collect::<Vec<i64>>())
            .unwrap_or_default()
    };
    let flag = |key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(false);
    Some(ChatFolderSpec {
        name,
        pinned_chat_ids: ids("pinned_chat_ids"),
        included_chat_ids: ids("included_chat_ids"),
        excluded_chat_ids: ids("excluded_chat_ids"),
        exclude_muted: flag("exclude_muted"),
        exclude_read: flag("exclude_read"),
        exclude_archived: flag("exclude_archived"),
        include_contacts: flag("include_contacts"),
        include_non_contacts: flag("include_non_contacts"),
        include_bots: flag("include_bots"),
        include_groups: flag("include_groups"),
        include_channels: flag("include_channels"),
    })
}

fn parse_message(value: &Value) -> Result<ParsedMessage, ParseError> {
    let (content, files) = parse_content(value.get("content"));
    Ok(ParsedMessage {
        id: MessageId(int53(value.get("id"))?),
        chat_id: ChatId(int53(value.get("chat_id"))?),
        is_outgoing: value
            .get("is_outgoing")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_pinned: value
            .get("is_pinned")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        media_album_id: int64(value.get("media_album_id")).unwrap_or(0),
        topic_id: parse_message_topic(value.get("topic_id")),
        content,
        files,
        reply_to: parse_reply_to(value.get("reply_to")),
        forward_info: parse_forward_info(value.get("forward_info")),
        interaction_info: parse_interaction_info(value.get("interaction_info")),
        reply_markup: parse_reply_markup(value.get("reply_markup")),
        self_destruct: parse_self_destruct(
            value.get("self_destruct_type"),
            value.get("self_destruct_in"),
        ),
    })
}

/// Parity slice 4: `message.topic_id` (TDLib 1.8.67, lines 3001–3010).
/// Returns the `forum_topic_id` for `messageTopicForum`; every other
/// variant (thread, direct messages, saved messages) and a missing/null
/// field map to `None` — Quill only models forum topics.
fn parse_message_topic(value: Option<&Value>) -> Option<i32> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    match value.get("@type").and_then(Value::as_str) {
        Some("messageTopicForum") => value
            .get("forum_topic_id")
            .and_then(Value::as_i64)
            .map(|id| id as i32),
        _ => None,
    }
}

fn parse_interaction_info(value: Option<&Value>) -> Option<MessageInteractionInfo> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    match value.get("@type").and_then(Value::as_str) {
        None | Some("messageInteractionInfo") => Some(MessageInteractionInfo {
            view_count: value.get("view_count").and_then(Value::as_i64).unwrap_or(0) as i32,
            forward_count: value
                .get("forward_count")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            reactions: parse_message_reactions(value.get("reactions")),
        }),
        _ => None,
    }
}

fn parse_message_reactions(value: Option<&Value>) -> Option<MessageReactions> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    match value.get("@type").and_then(Value::as_str) {
        None | Some("messageReactions") => {
            let reactions = value
                .get("reactions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(parse_message_reaction)
                .collect();
            Some(MessageReactions {
                reactions,
                are_tags: value
                    .get("are_tags")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        }
        _ => None,
    }
}

fn parse_message_reaction(value: &Value) -> Option<MessageReaction> {
    let reaction_type = parse_reaction_type(value.get("type"))?;
    Some(MessageReaction {
        reaction_type,
        total_count: value
            .get("total_count")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        is_chosen: value
            .get("is_chosen")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn parse_reaction_type(value: Option<&Value>) -> Option<ReactionType> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    match value.get("@type").and_then(Value::as_str) {
        Some("reactionTypeEmoji") => Some(ReactionType::Emoji {
            emoji: value
                .get("emoji")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        Some("reactionTypeCustomEmoji") => Some(ReactionType::CustomEmoji {
            custom_emoji_id: int64(value.get("custom_emoji_id")).unwrap_or(0),
        }),
        Some("reactionTypePaid") => Some(ReactionType::Paid),
        Some(_) => Some(ReactionType::Unknown),
        None => None,
    }
}

fn parse_forward_info(value: Option<&Value>) -> Option<MessageForwardInfo> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    match value.get("@type").and_then(Value::as_str) {
        Some("messageForwardInfo") => {
            let origin = parse_message_origin(value.get("origin"))?;
            Some(MessageForwardInfo {
                origin,
                date: value.get("date").and_then(Value::as_i64).unwrap_or(0) as i32,
            })
        }
        _ => None,
    }
}

fn parse_message_origin(value: Option<&Value>) -> Option<MessageOrigin> {
    let value = value?;
    match value.get("@type").and_then(Value::as_str) {
        Some("messageOriginUser") => Some(MessageOrigin::User {
            user_id: UserId(int53_or_zero(value.get("sender_user_id"))),
        }),
        Some("messageOriginHiddenUser") => Some(MessageOrigin::HiddenUser {
            sender_name: value
                .get("sender_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        Some("messageOriginChat") => Some(MessageOrigin::Chat {
            chat_id: ChatId(int53_or_zero(value.get("sender_chat_id"))),
            author_signature: value
                .get("author_signature")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        Some("messageOriginChannel") => Some(MessageOrigin::Channel {
            chat_id: ChatId(int53_or_zero(value.get("chat_id"))),
            message_id: MessageId(int53_or_zero(value.get("message_id"))),
            author_signature: value
                .get("author_signature")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        _ => None,
    }
}

fn parse_reply_to(value: Option<&Value>) -> Option<MessageReplyTo> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    match value.get("@type").and_then(Value::as_str) {
        Some("messageReplyToMessage") => {
            let message_id = int53_or_zero(value.get("message_id"));
            if message_id == 0 {
                return None;
            }
            let quote_text = value.get("quote").and_then(|quote| {
                if quote.is_null() {
                    return None;
                }
                let text = parse_formatted_text(quote.get("text"));
                if text.is_empty() { None } else { Some(text) }
            });
            let content_preview = value.get("content").and_then(|content| {
                if content.is_null() {
                    return None;
                }
                let preview = parse_content(Some(content)).0.preview();
                if preview.is_empty() {
                    None
                } else {
                    Some(preview)
                }
            });
            Some(MessageReplyTo {
                chat_id: ChatId(int53_or_zero(value.get("chat_id"))),
                message_id: MessageId(message_id),
                quote_text,
                content_preview,
            })
        }
        _ => None,
    }
}

fn parse_content(value: Option<&Value>) -> (MessageContent, Vec<ParsedFile>) {
    let Some(value) = value else {
        return (
            MessageContent::Unsupported {
                type_name: "missing".into(),
            },
            Vec::new(),
        );
    };
    match value.get("@type").and_then(Value::as_str) {
        Some("messageText") => parse_message_text(value),
        Some("messagePhoto") => parse_message_photo(value),
        Some("messageDocument") => parse_message_document(value),
        Some("messageSticker") => parse_message_sticker(value),
        Some("messageAnimation") => parse_message_animation(value),
        Some("messageVideo") => parse_message_video(value),
        Some("messageVideoNote") => parse_message_video_note(value),
        Some("messageVoiceNote") => parse_message_voice_note(value),
        Some("messageAudio") => parse_message_audio(value),
        Some("messagePoll") => parse_message_poll(value),
        Some("messageLocation") => parse_message_location(value),
        Some("messageLiveLocation") => parse_message_live_location(value),
        Some("messageVenue") => parse_message_venue(value),
        Some("messageContact") => parse_message_contact(value),
        Some("messageDice") => parse_message_dice(value),
        Some(other) => (
            MessageContent::Unsupported {
                type_name: other.to_string(),
            },
            Vec::new(),
        ),
        None => (
            MessageContent::Unsupported {
                type_name: "unknown".into(),
            },
            Vec::new(),
        ),
    }
}

fn parse_formatted_text(value: Option<&Value>) -> String {
    value
        .and_then(|text| text.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn parse_message_text(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let mut content = parse_text_content(value.get("text"));
    let (preview, files) = parse_link_preview(value.get("link_preview"));
    content.link_preview = preview.filter(LinkPreview::has_card);
    (MessageContent::Text(content), files)
}

fn parse_text_content(value: Option<&Value>) -> TextContent {
    let text = parse_formatted_text(value);
    TextContent {
        text: text.clone(),
        entities: parse_text_entities(&text, value),
        link_preview: None,
    }
}

/// Parse a `formattedText` caption into (text, entities). Captions carry the
/// same entity list as message text (Phase 4.1).
fn parse_caption(value: Option<&Value>) -> (String, Vec<TextEntity>) {
    let text = parse_formatted_text(value);
    let entities = parse_text_entities(&text, value);
    (text, entities)
}

/// `pollOption` (TDLib 1.8.67, `schema/td_api.tl:456`).
fn parse_poll_option(value: &Value) -> PollOption {
    PollOption {
        id: value
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        text: parse_formatted_text(value.get("text")),
        voter_count: value
            .get("voter_count")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        vote_percentage: value
            .get("vote_percentage")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        is_chosen: value
            .get("is_chosen")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

/// `pollType` (TDLib 1.8.67, `schema/td_api.tl:468` / `:475`).
fn parse_poll_type(value: Option<&Value>) -> PollType {
    match value.and_then(|v| v.get("@type")).and_then(Value::as_str) {
        Some("pollTypeQuiz") => PollType::Quiz {
            correct_option_ids: value
                .and_then(|v| v.get("correct_option_ids"))
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| id.as_i64())
                        .map(|n| n as i32)
                        .collect()
                })
                .unwrap_or_default(),
        },
        _ => PollType::Regular,
    }
}

/// `poll` (TDLib 1.8.67, `schema/td_api.tl:711`). `None` when the `poll`
/// object itself is missing or null.
fn parse_poll(value: Option<&Value>) -> Option<Poll> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    Some(Poll {
        id: int64(value.get("id")).unwrap_or(0),
        question: parse_formatted_text(value.get("question")),
        options: value
            .get("options")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(parse_poll_option)
            .collect(),
        total_voter_count: value
            .get("total_voter_count")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        is_anonymous: value
            .get("is_anonymous")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        allows_multiple_answers: value
            .get("allows_multiple_answers")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        allows_revoting: value
            .get("allows_revoting")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_closed: value
            .get("is_closed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        poll_type: parse_poll_type(value.get("type")),
    })
}

/// `messagePoll` (TDLib 1.8.67, `schema/td_api.tl:5241`).
fn parse_message_poll(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    match parse_poll(value.get("poll")) {
        Some(poll) => (
            MessageContent::Poll(PollContent {
                poll,
                description: parse_formatted_text(value.get("description")),
            }),
            Vec::new(),
        ),
        None => (
            MessageContent::Unsupported {
                type_name: "messagePoll".into(),
            },
            Vec::new(),
        ),
    }
}

/// `liveLocation` (TDLib 1.8.67, `schema/td_api.tl:653`). `None` when the
/// wrapper object itself is missing or null.
fn parse_live_location_state(value: Option<&Value>) -> Option<LiveLocationState> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    Some(LiveLocationState {
        live_period: value
            .get("live_period")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        expires_in: 0,
        heading: value.get("heading").and_then(Value::as_i64).unwrap_or(0) as i32,
        proximity_alert_radius: value
            .get("proximity_alert_radius")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
    })
}

/// `messageLocation` (TDLib 1.8.67, `schema/td_api.tl:5214`). An invalid
/// `location` (missing or failing the coordinate rule) yields
/// `Unsupported` so corrupt data never reaches a map link.
fn parse_message_location(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    match geo_location(value.get("location")) {
        Some(location) => (
            MessageContent::Location(LocationContent {
                location,
                live: None,
            }),
            Vec::new(),
        ),
        None => (
            MessageContent::Unsupported {
                type_name: "messageLocation".into(),
            },
            Vec::new(),
        ),
    }
}

/// `messageLiveLocation` (TDLib 1.8.67, `schema/td_api.tl:5211`).
/// `expires_in` rides on the message wrapper, `live_period` / `heading` /
/// `proximity_alert_radius` on the inner `liveLocation`.
fn parse_message_live_location(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let live_location = value.get("location");
    match (
        geo_location(live_location.and_then(|v| v.get("location"))),
        parse_live_location_state(live_location),
    ) {
        (Some(location), Some(mut live)) => {
            live.expires_in = value.get("expires_in").and_then(Value::as_i64).unwrap_or(0) as i32;
            (
                MessageContent::Location(LocationContent {
                    location,
                    live: Some(live),
                }),
                Vec::new(),
            )
        }
        _ => (
            MessageContent::Unsupported {
                type_name: "messageLiveLocation".into(),
            },
            Vec::new(),
        ),
    }
}

/// `messageVenue` (TDLib 1.8.67, `schema/td_api.tl:5217`). Provider `id`
/// and `type` are dropped (see `VenueContent` docs).
fn parse_message_venue(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let venue = value.get("venue");
    match geo_location(venue.and_then(|v| v.get("location"))) {
        Some(location) => (
            MessageContent::Venue(VenueContent {
                location,
                title: venue
                    .and_then(|v| v.get("title"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                address: venue
                    .and_then(|v| v.get("address"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                provider: venue
                    .and_then(|v| v.get("provider"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            }),
            Vec::new(),
        ),
        None => (
            MessageContent::Unsupported {
                type_name: "messageVenue".into(),
            },
            Vec::new(),
        ),
    }
}

/// `messageContact` (TDLib 1.8.67, `schema/td_api.tl:5220`). `user_id` is
/// int53 (0 when unknown); the vCard is kept verbatim for future use.
fn parse_message_contact(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let contact = value.get("contact");
    let phone_number = contact
        .and_then(|v| v.get("phone_number"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let first_name = contact
        .and_then(|v| v.get("first_name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let last_name = contact
        .and_then(|v| v.get("last_name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if contact.is_none_or(Value::is_null)
        || (phone_number.is_empty() && first_name.is_empty() && last_name.is_empty())
    {
        return (
            MessageContent::Unsupported {
                type_name: "messageContact".into(),
            },
            Vec::new(),
        );
    }
    (
        MessageContent::Contact(ContactContent {
            phone_number,
            first_name,
            last_name,
            vcard: contact
                .and_then(|v| v.get("vcard"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            user_id: contact
                .and_then(|v| int53(v.get("user_id")).ok())
                .unwrap_or(0),
        }),
        Vec::new(),
    )
}

/// `messageDice` (TDLib 1.8.67, `schema/td_api.tl:5231`). `initial_state` /
/// `final_state` (`DiceStickers`) and `success_animation_frame_number` are
/// dropped (see `DiceContent`); `emoji` falls back to 🎲 when empty.
///
/// **Safe rule:** `value` is the rolled number and is required — a missing,
/// non-integer, or out-of-`i32`-range `value` can't be displayed honestly,
/// so the whole message becomes `Unsupported` (`messageDice`) instead of
/// inventing (or wrapping) a number.
fn parse_message_dice(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let emoji = value
        .get("emoji")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let unsupported = (
        MessageContent::Unsupported {
            type_name: "messageDice".into(),
        },
        Vec::new(),
    );
    let Some(number) = value.get("value").and_then(Value::as_i64) else {
        return unsupported;
    };
    let Ok(value) = i32::try_from(number) else {
        return unsupported;
    };
    (
        MessageContent::Dice(DiceContent { emoji, value }),
        Vec::new(),
    )
}

/// Keep the entity types Quill renders (Phase 4.1): links plus the style
/// entities (`textEntityTypeBold` … `textEntityTypePreCode`). Unknown entity
/// types (mentions, hashtags, phone numbers, bank-card numbers, block
/// quotes, custom emoji, media timestamps, dates, …) are ignored.
fn parse_text_entities(text: &str, formatted: Option<&Value>) -> Vec<TextEntity> {
    let Some(entries) = formatted
        .and_then(|value| value.get("entities"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries {
        let offset = int53_or_zero(entry.get("offset"));
        let length = int53_or_zero(entry.get("length"));
        if length <= 0 || offset > i32::MAX as i64 || length > i32::MAX as i64 {
            continue;
        }
        let offset = offset as i32;
        let length = length as i32;
        let Ok(start) = utf16_to_utf8_offset(text, offset) else {
            continue;
        };
        let end_units = offset.saturating_add(length);
        let Ok(end) = utf16_to_utf8_offset(text, end_units) else {
            continue;
        };
        if start >= end || end > text.len() {
            continue;
        }
        let type_value = entry.get("type");
        let kind = match type_value
            .and_then(|t| t.get("@type"))
            .and_then(Value::as_str)
        {
            Some("textEntityTypeUrl") => TextEntityKind::Url,
            Some("textEntityTypeTextUrl") => TextEntityKind::TextUrl {
                url: type_value
                    .and_then(|t| t.get("url"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            },
            Some("textEntityTypeBold") => TextEntityKind::Bold,
            Some("textEntityTypeItalic") => TextEntityKind::Italic,
            Some("textEntityTypeUnderline") => TextEntityKind::Underline,
            Some("textEntityTypeStrikethrough") => TextEntityKind::Strikethrough,
            Some("textEntityTypeSpoiler") => TextEntityKind::Spoiler,
            Some("textEntityTypeCode") => TextEntityKind::Code,
            Some("textEntityTypePre") => TextEntityKind::Pre,
            Some("textEntityTypePreCode") => TextEntityKind::PreCode {
                language: type_value
                    .and_then(|t| t.get("language"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            },
            _ => continue,
        };
        out.push(TextEntity {
            utf8_start: start,
            utf8_end: end,
            kind,
        });
    }
    out
}

fn parse_link_preview(value: Option<&Value>) -> (Option<LinkPreview>, Vec<ParsedFile>) {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return (None, Vec::new());
    };
    if value.get("@type").and_then(Value::as_str) != Some("linkPreview") {
        return (None, Vec::new());
    }
    let (photo, files) = value
        .get("type")
        .filter(|preview_type| !preview_type.is_null())
        .map(parse_link_preview_photo)
        .unwrap_or((None, Vec::new()));
    (
        Some(LinkPreview {
            url: json_field_str(value, "url"),
            display_url: json_field_str(value, "display_url"),
            site_name: json_field_str(value, "site_name"),
            title: json_field_str(value, "title"),
            description: parse_formatted_text(value.get("description")),
            show_large_media: json_bool(value.get("show_large_media"), false),
            show_media_above_description: json_bool(
                value.get("show_media_above_description"),
                false,
            ),
            show_above_text: json_bool(value.get("show_above_text"), false),
            photo,
        }),
        files,
    )
}

/// Photo on `linkPreviewTypeArticle` / `Photo` (`photo`) and embedded players (`thumbnail` / `cover`).
fn parse_link_preview_photo(preview_type: &Value) -> (Option<PhotoContent>, Vec<ParsedFile>) {
    for key in ["photo", "thumbnail", "cover"] {
        let Some(candidate) = preview_type.get(key).filter(|value| !value.is_null()) else {
            continue;
        };
        let typed_photo = candidate.get("@type").and_then(Value::as_str) == Some("photo");
        if !typed_photo && candidate.get("sizes").and_then(Value::as_array).is_none() {
            continue;
        }
        let (sizes, files) = parse_photo_sizes(candidate);
        if sizes.is_empty() {
            continue;
        }
        return (
            Some(PhotoContent {
                caption: String::new(),
                caption_entities: Vec::new(),
                sizes,
                is_secret: false,
                has_spoiler: false,
            }),
            files,
        );
    }
    (None, Vec::new())
}

fn json_field_str(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn parse_message_photo(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let photo = value.get("photo");
    let (sizes, files) = photo.map(parse_photo_sizes).unwrap_or_default();
    let (caption, caption_entities) = parse_caption(value.get("caption"));
    (
        MessageContent::Photo(PhotoContent {
            caption,
            caption_entities,
            sizes,
            is_secret: value
                .get("is_secret")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            has_spoiler: value
                .get("has_spoiler")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        files,
    )
}

fn parse_message_document(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let document = value.get("document");
    let mut files = Vec::new();
    let file_id = match document.and_then(|d| parse_file(d.get("document")).ok()) {
        Some(file) => {
            let id = file.id;
            files.push(file);
            id
        }
        None => FileId(0),
    };
    if let Some(thumb) = document.and_then(|d| d.get("thumbnail"))
        && let Ok(thumb_file) = parse_file(thumb.get("file"))
    {
        files.push(thumb_file);
    }
    let (caption, caption_entities) = parse_caption(value.get("caption"));
    (
        MessageContent::Document(DocumentContent {
            file_name: document
                .and_then(|d| d.get("file_name"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            mime_type: document
                .and_then(|d| d.get("mime_type"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            caption,
            caption_entities,
            file_id,
        }),
        files,
    )
}

fn parse_message_sticker(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let sticker = value.get("sticker");
    let (item, mut files) = parse_sticker_value(sticker);
    let Some(item) = item else {
        return (
            MessageContent::Unsupported {
                type_name: "messageSticker".into(),
            },
            files,
        );
    };
    (
        MessageContent::Sticker(StickerContent {
            emoji: item.emoji,
            width: item.width,
            height: item.height,
            format: item.format,
            file_id: item.file_id,
            thumb_file_id: item.thumb_file_id,
            thumb_width: item.thumb_width,
            thumb_height: item.thumb_height,
            is_premium: value
                .get("is_premium")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        {
            files.retain(|file| file.id.0 != 0);
            files
        },
    )
}

fn parse_message_animation(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let animation = value.get("animation");
    let (item, mut files) = parse_animation_value(animation);
    let Some(item) = item else {
        return (
            MessageContent::Unsupported {
                type_name: "messageAnimation".into(),
            },
            files,
        );
    };
    files.retain(|file| file.id.0 != 0);
    let (caption, caption_entities) = parse_caption(value.get("caption"));
    (
        MessageContent::Animation(AnimationContent {
            duration: item.duration,
            width: item.width,
            height: item.height,
            file_name: item.file_name,
            mime_type: item.mime_type,
            caption,
            caption_entities,
            show_caption_above_media: value
                .get("show_caption_above_media")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            has_spoiler: value
                .get("has_spoiler")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_secret: value
                .get("is_secret")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            file_id: item.file_id,
            thumb_file_id: item.thumb_file_id,
            thumb_width: item.thumb_width,
            thumb_height: item.thumb_height,
        }),
        files,
    )
}

fn parse_message_video(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let (caption, caption_entities) = parse_caption(value.get("caption"));
    let video = value.get("video");
    if video
        .and_then(|video| video.get("@type"))
        .and_then(Value::as_str)
        != Some("video")
    {
        return (
            MessageContent::Unsupported {
                type_name: "messageVideo".into(),
            },
            Vec::new(),
        );
    }
    let video = video.expect("video");
    let mut files = Vec::new();
    let file_id = match parse_file(video.get("video")) {
        Ok(file) => {
            let id = file.id;
            files.push(file);
            id
        }
        Err(_) => FileId(0),
    };
    let thumb = video.get("thumbnail").filter(|thumb| !thumb.is_null());
    let (thumb_file_id, thumb_width, thumb_height) = if let Some(thumb) = thumb {
        let id = match parse_file(thumb.get("file")) {
            Ok(file) => {
                let id = file.id;
                files.push(file);
                Some(id)
            }
            Err(_) => None,
        };
        (
            id.filter(|id| id.0 != 0),
            int53_or_zero(thumb.get("width")) as i32,
            int53_or_zero(thumb.get("height")) as i32,
        )
    } else {
        (None, 0, 0)
    };
    files.retain(|file| file.id.0 != 0);
    (
        MessageContent::Video(VideoContent {
            duration: int53_or_zero(video.get("duration")) as i32,
            width: int53_or_zero(video.get("width")) as i32,
            height: int53_or_zero(video.get("height")) as i32,
            file_name: video
                .get("file_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            mime_type: video
                .get("mime_type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            caption,
            caption_entities,
            show_caption_above_media: value
                .get("show_caption_above_media")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            has_spoiler: value
                .get("has_spoiler")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_secret: value
                .get("is_secret")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            start_timestamp: int53_or_zero(value.get("start_timestamp")) as i32,
            supports_streaming: video
                .get("supports_streaming")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            has_stickers: video
                .get("has_stickers")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            file_id,
            thumb_file_id,
            thumb_width,
            thumb_height,
        }),
        files,
    )
}

fn parse_message_video_note(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let note = value.get("video_note");
    if note
        .and_then(|note| note.get("@type"))
        .and_then(Value::as_str)
        != Some("videoNote")
    {
        return (
            MessageContent::Unsupported {
                type_name: "messageVideoNote".into(),
            },
            Vec::new(),
        );
    }
    let note = note.expect("videoNote");
    let mut files = Vec::new();
    let file_id = match parse_file(note.get("video")) {
        Ok(file) => {
            let id = file.id;
            files.push(file);
            id
        }
        Err(_) => FileId(0),
    };
    let thumb = note.get("thumbnail").filter(|thumb| !thumb.is_null());
    let (thumb_file_id, thumb_width, thumb_height) = if let Some(thumb) = thumb {
        let id = match parse_file(thumb.get("file")) {
            Ok(file) => {
                let id = file.id;
                files.push(file);
                Some(id)
            }
            Err(_) => None,
        };
        (
            id.filter(|id| id.0 != 0),
            int53_or_zero(thumb.get("width")) as i32,
            int53_or_zero(thumb.get("height")) as i32,
        )
    } else {
        (None, 0, 0)
    };
    files.retain(|file| file.id.0 != 0);
    (
        MessageContent::VideoNote(VideoNoteContent {
            duration: int53_or_zero(note.get("duration")) as i32,
            waveform: parse_tdlib_bytes(note.get("waveform")),
            length: int53_or_zero(note.get("length")) as i32,
            is_viewed: value
                .get("is_viewed")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_secret: value
                .get("is_secret")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            file_id,
            thumb_file_id,
            thumb_width,
            thumb_height,
        }),
        files,
    )
}

fn parse_animation_value(value: Option<&Value>) -> (Option<AnimationItem>, Vec<ParsedFile>) {
    let Some(value) = value else {
        return (None, Vec::new());
    };
    if value.get("@type").and_then(Value::as_str) != Some("animation") {
        return (None, Vec::new());
    }
    let mut files = Vec::new();
    let file_id = match parse_file(value.get("animation")) {
        Ok(file) => {
            let id = file.id;
            files.push(file);
            id
        }
        Err(_) => FileId(0),
    };
    let thumb = value.get("thumbnail").filter(|thumb| !thumb.is_null());
    let (thumb_file_id, thumb_width, thumb_height) = if let Some(thumb) = thumb {
        let id = match parse_file(thumb.get("file")) {
            Ok(file) => {
                let id = file.id;
                files.push(file);
                Some(id)
            }
            Err(_) => None,
        };
        (
            id.filter(|id| id.0 != 0),
            int53_or_zero(thumb.get("width")) as i32,
            int53_or_zero(thumb.get("height")) as i32,
        )
    } else {
        (None, 0, 0)
    };
    (
        Some(AnimationItem {
            duration: int53_or_zero(value.get("duration")) as i32,
            width: int53_or_zero(value.get("width")) as i32,
            height: int53_or_zero(value.get("height")) as i32,
            file_name: value
                .get("file_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            mime_type: value
                .get("mime_type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            file_id,
            thumb_file_id,
            thumb_width,
            thumb_height,
        }),
        files,
    )
}

fn parse_animations(value: &Value) -> EnvelopePayload {
    let mut animations = Vec::new();
    let mut files = Vec::new();
    if let Some(entries) = value.get("animations").and_then(Value::as_array) {
        for entry in entries {
            let (item, item_files) = parse_animation_value(Some(entry));
            files.extend(item_files);
            if let Some(item) = item {
                animations.push(item);
            }
        }
    }
    files.retain(|file| file.id.0 != 0);
    EnvelopePayload::Animations { animations, files }
}

/// `sponsoredMessages` (TDLib 1.8.67). Unparseable rows are skipped, like
/// other vector payloads.
fn parse_sponsored_messages(value: &Value) -> Result<EnvelopePayload, ParseError> {
    let mut messages = Vec::new();
    let mut files = Vec::new();
    if let Some(entries) = value.get("messages").and_then(Value::as_array) {
        for entry in entries {
            let Ok((message, message_files)) = parse_sponsored_message(entry) else {
                continue;
            };
            messages.push(message);
            files.extend(message_files);
        }
    }
    files.retain(|file| file.id.0 != 0);
    Ok(EnvelopePayload::SponsoredMessages {
        messages,
        files,
        messages_between: value
            .get("messages_between")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
    })
}

fn parse_sponsored_message(
    entry: &Value,
) -> Result<(SponsoredMessage, Vec<ParsedFile>), ParseError> {
    if entry.get("@type").and_then(Value::as_str) != Some("sponsoredMessage") {
        return Err(ParseError::MissingField);
    }
    let (content, mut files) = parse_content(entry.get("content"));
    let (sponsor, sponsor_files) = parse_advertisement_sponsor(entry.get("sponsor"));
    files.extend(sponsor_files);
    Ok((
        SponsoredMessage {
            message_id: int53(entry.get("message_id"))?,
            is_recommended: json_bool(entry.get("is_recommended"), false),
            can_be_reported: json_bool(entry.get("can_be_reported"), false),
            content,
            sponsor,
            title: json_field_str(entry, "title"),
            button_text: json_field_str(entry, "button_text"),
            additional_info: json_field_str(entry, "additional_info"),
        },
        files,
    ))
}

/// `advertisementSponsor` (TDLib 1.8.67). A null sponsor is an empty sponsor,
/// not an error.
fn parse_advertisement_sponsor(value: Option<&Value>) -> (AdvertisementSponsor, Vec<ParsedFile>) {
    let empty = (
        AdvertisementSponsor {
            url: String::new(),
            photo: None,
            info: String::new(),
        },
        Vec::new(),
    );
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return empty;
    };
    if value.get("@type").and_then(Value::as_str) != Some("advertisementSponsor") {
        return empty;
    }
    let (photo, files) = value
        .get("photo")
        .filter(|photo| !photo.is_null())
        .map(parse_sponsored_photo)
        .unwrap_or((None, Vec::new()));
    (
        AdvertisementSponsor {
            url: json_field_str(value, "url"),
            photo,
            info: json_field_str(value, "info"),
        },
        files,
    )
}

/// Sponsor photo is schema `photo`. Same shape as link-preview photos.
fn parse_sponsored_photo(photo: &Value) -> (Option<PhotoContent>, Vec<ParsedFile>) {
    let typed_photo = photo.get("@type").and_then(Value::as_str) == Some("photo");
    if !typed_photo && photo.get("sizes").and_then(Value::as_array).is_none() {
        return (None, Vec::new());
    }
    let (sizes, files) = parse_photo_sizes(photo);
    if sizes.is_empty() {
        return (None, Vec::new());
    }
    (
        Some(PhotoContent {
            caption: String::new(),
            caption_entities: Vec::new(),
            sizes,
            is_secret: false,
            has_spoiler: false,
        }),
        files,
    )
}

/// `reportOption` rows (`reportSponsoredResultOptionRequired.options`).
fn parse_report_options(value: Option<&Value>) -> Vec<ReportOption> {
    let mut options = Vec::new();
    let Some(entries) = value.and_then(Value::as_array) else {
        return options;
    };
    for entry in entries {
        if entry.get("@type").and_then(Value::as_str) != Some("reportOption") {
            continue;
        }
        options.push(ReportOption {
            id: entry
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            text: json_field_str(entry, "text"),
        });
    }
    options
}

fn parse_message_audio(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let (caption, caption_entities) = parse_caption(value.get("caption"));
    let audio = value.get("audio");
    if audio
        .and_then(|audio| audio.get("@type"))
        .and_then(Value::as_str)
        != Some("audio")
    {
        return (
            MessageContent::Unsupported {
                type_name: "messageAudio".into(),
            },
            Vec::new(),
        );
    }
    let audio = audio.expect("audio");
    let mut files = Vec::new();
    let file_id = match parse_file(audio.get("audio")) {
        Ok(file) => {
            let id = file.id;
            files.push(file);
            id
        }
        Err(_) => FileId(0),
    };
    let album_cover_thumbnail = audio
        .get("album_cover_thumbnail")
        .filter(|thumb| !thumb.is_null())
        .and_then(|thumb| parse_album_cover_thumb(thumb, &mut files));
    let mut external_album_covers = Vec::new();
    if let Some(entries) = audio.get("external_album_covers").and_then(Value::as_array) {
        for entry in entries {
            if let Some(cover) = parse_album_cover_thumb(entry, &mut files) {
                external_album_covers.push(cover);
            }
        }
    }
    files.retain(|file| file.id.0 != 0);
    (
        MessageContent::Audio(AudioContent {
            duration: int53_or_zero(audio.get("duration")) as i32,
            title: json_field_str(audio, "title"),
            performer: json_field_str(audio, "performer"),
            file_name: json_field_str(audio, "file_name"),
            mime_type: json_field_str(audio, "mime_type"),
            caption,
            caption_entities,
            album_cover_minithumbnail: parse_minithumbnail(audio.get("album_cover_minithumbnail")),
            album_cover_thumbnail,
            external_album_covers,
            file_id,
        }),
        files,
    )
}

fn parse_minithumbnail(value: Option<&Value>) -> Option<MiniThumbnail> {
    let value = value.filter(|value| !value.is_null())?;
    if value.get("@type").and_then(Value::as_str) != Some("minithumbnail") {
        return None;
    }
    let data = parse_tdlib_bytes(value.get("data"));
    if data.is_empty() {
        return None;
    }
    Some(MiniThumbnail {
        width: int53_or_zero(value.get("width")) as i32,
        height: int53_or_zero(value.get("height")) as i32,
        data,
    })
}

fn parse_album_cover_thumb(value: &Value, files: &mut Vec<ParsedFile>) -> Option<AlbumCoverThumb> {
    if value.get("@type").and_then(Value::as_str) != Some("thumbnail") {
        return None;
    }
    let file = parse_file(value.get("file")).ok()?;
    if file.id.0 == 0 {
        return None;
    }
    let cover = AlbumCoverThumb {
        width: int53_or_zero(value.get("width")) as i32,
        height: int53_or_zero(value.get("height")) as i32,
        file_id: file.id,
    };
    files.push(file);
    Some(cover)
}

fn parse_message_voice_note(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let (caption, caption_entities) = parse_caption(value.get("caption"));
    let voice_note = value.get("voice_note");
    let mut files = Vec::new();
    let file_id = match voice_note.and_then(|note| parse_file(note.get("voice")).ok()) {
        Some(file) => {
            let id = file.id;
            files.push(file);
            id
        }
        None => FileId(0),
    };
    let duration = voice_note
        .and_then(|note| note.get("duration"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .clamp(0, i64::from(i32::MAX)) as i32;
    (
        MessageContent::VoiceNote(VoiceNoteContent {
            duration,
            waveform: parse_tdlib_bytes(voice_note.and_then(|note| note.get("waveform"))),
            mime_type: voice_note
                .and_then(|note| note.get("mime_type"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            caption,
            caption_entities,
            is_listened: value
                .get("is_listened")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            file_id,
        }),
        files,
    )
}

/// TDLib JSON `bytes` is a base64 string (empty when the waveform is unknown).
fn parse_tdlib_bytes(value: Option<&Value>) -> Vec<u8> {
    let Some(text) = value.and_then(Value::as_str) else {
        return Vec::new();
    };
    if text.is_empty() {
        return Vec::new();
    }
    STANDARD.decode(text).unwrap_or_default()
}

fn parse_sticker_format(value: Option<&Value>) -> StickerFormat {
    match value.and_then(|v| v.get("@type")).and_then(Value::as_str) {
        Some("stickerFormatWebp") => StickerFormat::Webp,
        Some("stickerFormatTgs") => StickerFormat::Tgs,
        Some("stickerFormatWebm") => StickerFormat::Webm,
        _ => StickerFormat::Unknown,
    }
}

fn parse_sticker_value(value: Option<&Value>) -> (Option<StickerItem>, Vec<ParsedFile>) {
    let Some(value) = value else {
        return (None, Vec::new());
    };
    if value.get("@type").and_then(Value::as_str) != Some("sticker") {
        return (None, Vec::new());
    }
    let mut files = Vec::new();
    let file_id = match parse_file(value.get("sticker")) {
        Ok(file) => {
            let id = file.id;
            files.push(file);
            id
        }
        Err(_) => FileId(0),
    };
    let thumb = value.get("thumbnail").filter(|thumb| !thumb.is_null());
    let (thumb_file_id, thumb_width, thumb_height) = if let Some(thumb) = thumb {
        let id = match parse_file(thumb.get("file")) {
            Ok(file) => {
                let id = file.id;
                files.push(file);
                Some(id)
            }
            Err(_) => None,
        };
        (
            id,
            int53_or_zero(thumb.get("width")) as i32,
            int53_or_zero(thumb.get("height")) as i32,
        )
    } else {
        (None, 0, 0)
    };
    (
        Some(StickerItem {
            id: int64(value.get("id")).unwrap_or(0),
            set_id: int64(value.get("set_id")).unwrap_or(0),
            emoji: value
                .get("emoji")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            width: int53_or_zero(value.get("width")) as i32,
            height: int53_or_zero(value.get("height")) as i32,
            format: parse_sticker_format(value.get("format")),
            file_id,
            thumb_file_id,
            thumb_width,
            thumb_height,
        }),
        files,
    )
}

fn parse_sticker_set_info(value: &Value) -> Option<StickerSetInfo> {
    if value.get("@type").and_then(Value::as_str) != Some("stickerSetInfo") {
        return None;
    }
    Some(StickerSetInfo {
        id: int64(value.get("id")).unwrap_or(0),
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        size: int53_or_zero(value.get("size")) as i32,
        is_installed: value
            .get("is_installed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_official: value
            .get("is_official")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn parse_sticker_sets(value: &Value) -> EnvelopePayload {
    let sets = value
        .get("sets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(parse_sticker_set_info)
        .collect();
    EnvelopePayload::StickerSets {
        total_count: int53_or_zero(value.get("total_count")) as i32,
        sets,
    }
}

fn parse_sticker_set(value: &Value) -> EnvelopePayload {
    let mut stickers = Vec::new();
    let mut files = Vec::new();
    if let Some(entries) = value.get("stickers").and_then(Value::as_array) {
        for entry in entries {
            let (item, item_files) = parse_sticker_value(Some(entry));
            if let Some(item) = item {
                stickers.push(item);
            }
            files.extend(item_files);
        }
    }
    EnvelopePayload::StickerSet {
        id: int64(value.get("id")).unwrap_or(0),
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        stickers,
        files,
    }
}

/// Phase 6: preferred profile-photo file from a `userFullInfo` (or the
/// nested `user_full_info` of an `updateUserFullInfo`) object's
/// `photo:chatPhoto` (schema 1.8.67, lines 1030 and 2468). Reuses
/// `parse_photo_sizes`; prefers `type == "m"`, else the largest size up to
/// 320px wide, else the smallest size. `None` when there is no photo or
/// the constructor is not a `chatPhoto`.
fn parse_user_full_info_photo(value: &Value) -> Option<ParsedFile> {
    let photo = value.get("photo")?;
    if photo.get("@type").and_then(Value::as_str) != Some("chatPhoto") {
        return None;
    }
    let (sizes, files) = parse_photo_sizes(photo);
    let pick = sizes
        .iter()
        .find(|size| size.type_name == "m")
        .or_else(|| {
            sizes
                .iter()
                .filter(|size| size.width > 0 && size.width <= 320)
                .max_by_key(|size| size.width)
        })
        .or_else(|| sizes.iter().min_by_key(|size| (size.width, size.height)))?;
    files.into_iter().find(|file| file.id == pick.file_id)
}

/// Parity slice: the `small` file from a `chatPhotoInfo` (`chat.photo` /
/// `updateChatPhoto.photo`, schema 1.8.67, lines 762 and 10488). The small
/// variant is the cheap thumbnail the chat list renders; `big` is not
/// kept. Null/absent/malformed → `None`.
fn parse_chat_photo_small(value: Option<&Value>) -> Option<ParsedFile> {
    let photo = value.filter(|v| !v.is_null())?;
    parse_file(photo.get("small")).ok()
}

fn parse_photo_sizes(photo: &Value) -> (Vec<PhotoSizeView>, Vec<ParsedFile>) {
    let mut files = Vec::new();
    let mut sizes = Vec::new();
    if let Some(entries) = photo.get("sizes").and_then(Value::as_array) {
        for entry in entries {
            let Ok(file) = parse_file(entry.get("photo")) else {
                continue;
            };
            sizes.push(PhotoSizeView {
                type_name: entry
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                width: int53_or_zero(entry.get("width")) as i32,
                height: int53_or_zero(entry.get("height")) as i32,
                file_id: file.id,
            });
            files.push(file);
        }
    }
    (sizes, files)
}

fn parse_file(value: Option<&Value>) -> Result<ParsedFile, ParseError> {
    let value = value.ok_or(ParseError::MissingField)?;
    let id = i32::try_from(int53(value.get("id"))?).map_err(|_| ParseError::BadInt)?;
    let local = value.get("local");
    Ok(ParsedFile {
        id: FileId(id),
        size: int53_or_zero(value.get("size")),
        expected_size: int53_or_zero(value.get("expected_size")),
        local: LocalFileState {
            path: local
                .and_then(|l| l.get("path"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            can_be_downloaded: local
                .and_then(|l| l.get("can_be_downloaded"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_downloading_active: local
                .and_then(|l| l.get("is_downloading_active"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_downloading_completed: local
                .and_then(|l| l.get("is_downloading_completed"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
    })
}

fn parse_error(value: Option<&Value>) -> TdError {
    TdError::from_code(
        value
            .and_then(|v| v.get("code"))
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
    )
}

fn int53(value: Option<&Value>) -> Result<i64, ParseError> {
    match value {
        Some(Value::Number(n)) => n.as_i64().ok_or(ParseError::BadInt),
        Some(Value::String(s)) => s.parse().map_err(|_| ParseError::BadInt),
        _ => Err(ParseError::MissingField),
    }
}

fn int53_or_zero(value: Option<&Value>) -> i64 {
    int53(value).unwrap_or(0)
}

fn int64(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::String(s)) => s.parse().ok(),
        Some(Value::Number(n)) => n.as_i64(),
        _ => None,
    }
}

fn int53_array(value: Option<&Value>) -> Vec<MessageId> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| int53(Some(v)).ok())
        .map(MessageId)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    InvalidJson,
    MissingField,
    BadInt,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_variant_does_not_keep_raw_json() {
        let json =
            r#"{"@type":"updateSomethingSecret","secret":"CANARY_PHONE_+1555","@extra":"1"}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::Unknown(ref kind) => {
                assert_eq!(kind.type_name, "updateSomethingSecret")
            }
            other => panic!("unexpected {other:?}"),
        }
        let debug = format!("{env:?}");
        assert!(!debug.contains("CANARY_PHONE"));
        assert!(!debug.contains("+1555"));
    }

    /// Phase B1: `updateSecretChat` parses the full `secretChat` record —
    /// all three states plus the base64 `key_hash` bytes, `is_outbound`,
    /// and `layer`.
    #[test]
    fn secret_chat_states_parsed_with_key_hash() {
        let key_hash_b64 = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIj";
        for (state_type, expected) in [
            ("secretChatStatePending", SecretChatState::Pending),
            ("secretChatStateReady", SecretChatState::Ready),
            ("secretChatStateClosed", SecretChatState::Closed),
        ] {
            let json = format!(
                r#"{{"@type":"updateSecretChat","secret_chat":{{"@type":"secretChat","id":7,"user_id":41,"state":{{"@type":"{state_type}"}},"is_outbound":true,"key_hash":"{key_hash_b64}","layer":144}}}}"#
            );
            let env = parse_envelope(&json).unwrap();
            match env.payload {
                EnvelopePayload::UpdateSecretChat { secret_chat } => {
                    assert_eq!(secret_chat.id, 7);
                    assert_eq!(secret_chat.user_id, 41);
                    assert_eq!(secret_chat.state, expected);
                    assert!(secret_chat.is_outbound);
                    assert_eq!(secret_chat.key_hash.len(), 36);
                    assert_eq!(secret_chat.key_hash[0], 0x00);
                    assert_eq!(secret_chat.key_hash[35], 0x23);
                    assert_eq!(secret_chat.layer, 144);
                }
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    /// Phase B2: `Debug` on `ParsedSecretChat` redacts the key bytes
    /// (length only) — a stray `{env:?}` in a log can never leak key
    /// material.
    #[test]
    fn secret_chat_debug_redacts_key_hash() {
        let secret_chat = ParsedSecretChat {
            id: 7,
            user_id: 41,
            state: SecretChatState::Ready,
            is_outbound: true,
            key_hash: vec![0xAB; 36],
            layer: 144,
        };
        let debug = format!("{secret_chat:?}");
        assert!(debug.contains("key_hash_len: 36"));
        // 0xAB = 171; a full-bytes Debug would print it 36 times.
        assert!(!debug.contains("171"));
    }

    /// Phase B1: an unknown `SecretChatState` constructor degrades to
    /// `SecretChatState::Unknown` instead of failing the envelope parse.
    #[test]
    fn secret_chat_unknown_state_degrades() {
        let env = parse_envelope(
            r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":7,"user_id":41,"state":{"@type":"secretChatStateFuture"},"is_outbound":false,"key_hash":"","layer":144}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateSecretChat { secret_chat } => {
                assert_eq!(
                    secret_chat.state,
                    SecretChatState::Unknown("secretChatStateFuture".to_string())
                );
                assert!(secret_chat.key_hash.is_empty());
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    /// Phase B3: `message.self_destruct_type` / `message.self_destruct_in`
    /// parse (schema 1.8.67 lines 3146–3147 / 3165 / 5915 / 5918); unknown
    /// future variants degrade to `None`; absent/null fields mean no timer.
    #[test]
    fn self_destruct_type_and_in_parsed() {
        let base = |sd_type: &str, sd_in: &str| {
            format!(
                r#"{{"id":1,"chat_id":41,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}},"self_destruct_type":{sd_type},"self_destruct_in":{sd_in}}}"#
            )
        };
        let parse = |json: &str| {
            let value: Value = serde_json::from_str(json).unwrap();
            parse_message(&value).unwrap()
        };

        let timer = parse(&base(
            r#"{"@type":"messageSelfDestructTypeTimer","self_destruct_time":60}"#,
            "42.5",
        ));
        let sd = timer.self_destruct.expect("timer parsed");
        assert_eq!(sd.kind, SelfDestructKind::Timer { secs: 60 });
        assert_eq!(sd.expires_in_ms, 42_500);

        let immediate = parse(&base(
            r#"{"@type":"messageSelfDestructTypeImmediately"}"#,
            "0",
        ));
        let sd = immediate.self_destruct.expect("immediately parsed");
        assert_eq!(sd.kind, SelfDestructKind::Immediately);
        assert_eq!(sd.remaining_secs(sd.fetched_at_ms), None);

        let plain = parse(&base("null", "0"));
        assert_eq!(plain.self_destruct, None);

        let future = parse(&base(
            r#"{"@type":"messageSelfDestructTypeFuture"}"#,
            "10.0",
        ));
        assert_eq!(future.self_destruct, None);

        // `remaining_secs` decays locally; garbage `self_destruct_in`
        // values degrade to "not scheduled".
        let sd = timer.self_destruct.unwrap();
        assert_eq!(sd.remaining_secs(sd.fetched_at_ms), Some(43));
        assert_eq!(sd.remaining_secs(sd.fetched_at_ms + 42_500), Some(0));
        assert_eq!(sd.badge_label(sd.fetched_at_ms), "⏱ 43s left");
        let never = parse(&base(
            r#"{"@type":"messageSelfDestructTypeTimer","self_destruct_time":60}"#,
            "0",
        ));
        assert_eq!(
            never
                .self_destruct
                .unwrap()
                .badge_label(never.self_destruct.unwrap().fetched_at_ms),
            "⏱ 60s"
        );
        let nan = parse(&base(
            r#"{"@type":"messageSelfDestructTypeTimer","self_destruct_time":60}"#,
            "-3.0",
        ));
        assert_eq!(nan.self_destruct.unwrap().remaining_secs(u64::MAX), None);
    }

    #[test]
    fn int64_order_is_not_float() {
        let json = r#"{"@type":"updateChatPosition","chat_id":42,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9223372036854775806","is_pinned":false}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateChatPosition(pos) => {
                assert_eq!(pos.order, 9223372036854775806);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn chat_folder_spec_parsed() {
        // Parity slice: `getChatFolder` response (schema 1.8.67 line 13355)
        // carries the full `chatFolder` spec (line 3476).
        let env = parse_envelope(
            r#"{"@type":"chatFolder","name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]},"animate_custom_emoji":false},"icon":null,"color_id":-1,"is_shareable":false,"pinned_chat_ids":[11],"included_chat_ids":[12,13],"excluded_chat_ids":[14],"exclude_muted":true,"exclude_read":false,"exclude_archived":true,"include_contacts":true,"include_non_contacts":false,"include_bots":true,"include_groups":false,"include_channels":true}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::ChatFolder { spec } => {
                assert_eq!(spec.name, "Work");
                assert_eq!(spec.pinned_chat_ids, vec![11]);
                assert_eq!(spec.included_chat_ids, vec![12, 13]);
                assert_eq!(spec.excluded_chat_ids, vec![14]);
                assert!(spec.exclude_muted);
                assert!(!spec.exclude_read);
                assert!(spec.exclude_archived);
                assert!(spec.include_contacts);
                assert!(!spec.include_non_contacts);
                assert!(spec.include_bots);
                assert!(!spec.include_groups);
                assert!(spec.include_channels);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn chat_folder_info_response_parsed() {
        // Parity slice: `createChatFolder` / `editChatFolder` responses
        // (schema 1.8.67 lines 13358 / 13361) are `chatFolderInfo`.
        let env = parse_envelope(
            r#"{"@type":"chatFolderInfo","id":5,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"New","entities":[]},"animate_custom_emoji":false},"icon":null,"color_id":-1,"is_shareable":false,"has_my_invite_links":false}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::ChatFolderInfo(info) => {
                assert_eq!(info.id, 5);
                assert_eq!(info.name, "New");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn chat_lists_response_parsed() {
        // Parity slice: `getChatListsToAddChat` response (schema 1.8.67
        // line 13347) is `chatLists`.
        let env = parse_envelope(
            r#"{"@type":"chatLists","chat_lists":[{"@type":"chatListMain"},{"@type":"chatListFolder","chat_folder_id":2}]}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::ChatLists { lists } => {
                assert_eq!(lists, vec![ChatList::Main, ChatList::Folder(2)]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn send_message_uses_topic_id_field_in_schema() {
        // Guard against obsolete message_thread_id examples.
        let schema = include_str!("../../schema/td_api.tl");
        let send = schema
            .lines()
            .find(|l| l.starts_with("sendMessage "))
            .expect("sendMessage");
        assert!(send.contains("topic_id:MessageTopic"));
        assert!(!send.contains("message_thread_id"));
    }

    #[test]
    fn parse_message_topic_forum_yields_forum_topic_id() {
        // Parity slice 4: `message.topic_id` (schema 1.8.67, lines 3001–3010
        // and 3165) — only `messageTopicForum` maps to `Some`.
        let forum = parse_envelope(
            r#"{"@type":"message","id":7,"chat_id":16,"is_outgoing":false,"topic_id":{"@type":"messageTopicForum","forum_topic_id":2},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}"#,
        )
        .unwrap();
        match forum.payload {
            EnvelopePayload::Message(message) => assert_eq!(message.topic_id, Some(2)),
            other => panic!("{other:?}"),
        }
        let thread = parse_envelope(
            r#"{"@type":"message","id":8,"chat_id":16,"is_outgoing":false,"topic_id":{"@type":"messageTopicThread","message_thread_id":5},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}"#,
        )
        .unwrap();
        match thread.payload {
            EnvelopePayload::Message(message) => assert_eq!(message.topic_id, None),
            other => panic!("{other:?}"),
        }
        let plain = parse_envelope(
            r#"{"@type":"message","id":9,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}"#,
        )
        .unwrap();
        match plain.payload {
            EnvelopePayload::Message(message) => assert_eq!(message.topic_id, None),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn update_new_chat_parses_send_permission() {
        // Parity slice 4: `chat.permissions.can_send_basic_messages`
        // (schema 1.8.67, line 1070).
        let env = parse_envelope(
            r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Demo forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"permissions":{"@type":"chatPermissions","can_send_basic_messages":false},"unread_count":0}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewChat {
                can_send_basic_messages,
                ..
            } => assert!(!can_send_basic_messages),
            other => panic!("{other:?}"),
        }
        // Absent block defaults to true (lenient parsing).
        let env = parse_envelope(
            r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Demo forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewChat {
                can_send_basic_messages,
                ..
            } => assert!(can_send_basic_messages),
            other => panic!("{other:?}"),
        }
        // `updateChatPermissions` (schema 1.8.67, line 10500).
        let env = parse_envelope(
            r#"{"@type":"updateChatPermissions","chat_id":16,"permissions":{"@type":"chatPermissions","can_send_basic_messages":true}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateChatPermissions {
                chat_id,
                can_send_basic_messages,
            } => {
                assert_eq!(chat_id, ChatId(16));
                assert!(can_send_basic_messages);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn forum_topics_parse_keeps_needed_fields() {
        let json = r#"{"@type":"forumTopics","@extra":"9","total_count":2,"topics":[{"info":{"@type":"forumTopicInfo","chat_id":16,"forum_topic_id":1,"name":"General","icon":{"@type":"forumTopicIcon","color":7322096,"custom_emoji_id":"0"},"creation_date":1700000000,"creator_id":{"@type":"messageSenderUser","user_id":5},"is_general":true,"is_outgoing":false,"is_closed":false,"is_hidden":false,"is_name_implicit":false},"last_message":{"id":50,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_TOPIC_welcome","entities":[]}}},"order":"500","is_pinned":true,"unread_count":3,"last_read_inbox_message_id":50,"last_read_outbox_message_id":0,"unread_mention_count":0,"unread_reaction_count":0,"unread_poll_vote_count":0,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":true,"mute_for":0,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":true,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false},"draft_message":null},{"info":{"@type":"forumTopicInfo","chat_id":16,"forum_topic_id":2,"name":"Random","icon":{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"},"creation_date":1700000100,"creator_id":{"@type":"messageSenderUser","user_id":6},"is_general":false,"is_outgoing":false,"is_closed":true,"is_hidden":false,"is_name_implicit":false},"last_message":null,"order":"100","is_pinned":false,"unread_count":0,"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"unread_mention_count":0,"unread_reaction_count":0,"unread_poll_vote_count":0,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":true,"mute_for":0,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":true,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false},"draft_message":null}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::ForumTopics {
                total_count,
                topics,
            } => {
                assert_eq!(total_count, 2);
                assert_eq!(topics.len(), 2);
                let general = &topics[0];
                assert_eq!(general.forum_topic_id, 1);
                assert_eq!(general.name, "General");
                assert!(general.is_general);
                assert!(!general.is_closed);
                assert!(general.is_pinned);
                assert_eq!(general.unread_count, 3);
                assert_eq!(general.order, 500);
                assert_eq!(general.last_message_preview, "CANARY_TOPIC_welcome");
                let random = &topics[1];
                assert_eq!(random.forum_topic_id, 2);
                assert!(!random.is_general);
                assert!(random.is_closed);
                assert_eq!(random.last_message_preview, "");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn forum_topic_without_info_is_skipped() {
        let json = r#"{"@type":"forumTopics","total_count":1,"topics":[{"order":"1"}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::ForumTopics { topics, .. } => assert!(topics.is_empty()),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn update_supergroup_parses_admin_restrict_right() {
        // Phase A1: `chatMemberStatusAdministrator` carries `rights`
        // (schema 1.8.67 line 2500); `can_restrict_members` (line 1092) is
        // what `setChatSlowModeDelay` requires (line 13551).
        let json = r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":25,"is_forum":false,"status":{"@type":"chatMemberStatusAdministrator","can_be_edited":false,"rights":{"@type":"chatAdministratorRights","can_manage_chat":false,"can_change_info":false,"can_post_messages":false,"can_edit_messages":false,"can_delete_messages":false,"can_invite_users":false,"can_restrict_members":true,"can_pin_messages":false,"can_promote_members":false,"can_manage_video_chats":false,"can_post_stories":false,"can_edit_stories":false,"can_delete_stories":false,"can_manage_direct_messages":false,"can_manage_tags":false,"can_send_welcome_messages":false,"is_anonymous":false}}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateSupergroup {
                status,
                can_restrict_members,
                ..
            } => {
                assert_eq!(status, ChannelMemberStatus::Administrator);
                assert_eq!(can_restrict_members, Some(true));
            }
            other => panic!("unexpected {other:?}"),
        }
        // Admin without the right → `Some(false)`.
        let json = r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":25,"is_forum":false,"status":{"@type":"chatMemberStatusAdministrator","can_be_edited":false,"rights":{"@type":"chatAdministratorRights","can_restrict_members":false}}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateSupergroup {
                can_restrict_members,
                ..
            } => assert_eq!(can_restrict_members, Some(false)),
            other => panic!("unexpected {other:?}"),
        }
        // Admin with no rights block → `None` (treated as lacking the right).
        let json = r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":25,"is_forum":false,"status":{"@type":"chatMemberStatusAdministrator"}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateSupergroup {
                can_restrict_members,
                ..
            } => assert_eq!(can_restrict_members, None),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn update_supergroup_parses_forum_flag() {
        let json = r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"usernames":null,"date":1700000000,"status":{"@type":"chatMemberStatusMember"},"member_count":42,"boost_level":0,"has_automatic_translation":false,"has_linked_chat":false,"has_location":false,"sign_messages":false,"show_message_sender":false,"join_to_send_messages":false,"join_by_request":false,"is_slow_mode_enabled":false,"is_channel":false,"is_broadcast_group":false,"is_forum":true,"is_direct_messages_group":false,"is_administered_direct_messages_group":false,"verification_status":{"@type":"verificationStatus","is_verified":false,"is_scam":false,"is_fake":false},"has_direct_messages_group":false,"has_forum_tabs":false,"restriction_info":null,"paid_message_star_count":0,"active_story_state":null}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateSupergroup {
                supergroup_id,
                is_forum,
                username,
                status,
                can_restrict_members,
            } => {
                assert_eq!(supergroup_id, 16);
                assert!(is_forum);
                // Parity slice: null `usernames` → empty username.
                assert_eq!(username, "");
                // Phase A1: own status parsed (`chatMemberStatusMember`).
                assert_eq!(status, ChannelMemberStatus::Member);
                // Members carry no admin rights.
                assert_eq!(can_restrict_members, None);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn update_supergroup_parses_username() {
        // Parity slice: first active username is kept for the
        // channel/supergroup header (schema 1.8.67 lines 2746/2372).
        let json = r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":18,"usernames":{"@type":"usernames","active_usernames":["demochannel","backupname"],"disabled_usernames":[],"editable_username":"demochannel","collectible_usernames":[]},"is_forum":false}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateSupergroup {
                supergroup_id,
                is_forum,
                username,
                status,
                can_restrict_members,
            } => {
                assert_eq!(supergroup_id, 18);
                assert!(!is_forum);
                assert_eq!(username, "demochannel");
                assert_eq!(status, ChannelMemberStatus::Unknown);
                assert_eq!(can_restrict_members, None);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn supergroup_response_parses_forum_flag() {
        let json = r#"{"@type":"supergroup","@extra":"4","id":17,"is_forum":false}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::Supergroup {
                supergroup_id,
                is_forum,
                username,
                status,
                can_restrict_members,
            } => {
                assert_eq!(supergroup_id, 17);
                assert!(!is_forum);
                assert_eq!(username, "");
                assert_eq!(status, ChannelMemberStatus::Unknown);
                assert_eq!(can_restrict_members, None);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn read_inbox_and_outbox_are_typed() {
        let inbox = parse_envelope(
            r#"{"@type":"updateChatReadInbox","chat_id":4,"last_read_inbox_message_id":88,"unread_count":3}"#,
        )
        .unwrap();
        match inbox.payload {
            EnvelopePayload::UpdateChatReadInbox {
                chat_id,
                last_read_inbox_message_id,
                unread_count,
            } => {
                assert_eq!(chat_id.0, 4);
                assert_eq!(last_read_inbox_message_id.0, 88);
                assert_eq!(unread_count, 3);
            }
            other => panic!("{other:?}"),
        }
        let outbox = parse_envelope(
            r#"{"@type":"updateChatReadOutbox","chat_id":4,"last_read_outbox_message_id":91}"#,
        )
        .unwrap();
        match outbox.payload {
            EnvelopePayload::UpdateChatReadOutbox {
                chat_id,
                last_read_outbox_message_id,
            } => {
                assert_eq!(chat_id.0, 4);
                assert_eq!(last_read_outbox_message_id.0, 91);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn user_full_info_bot_info_parsed() {
        // `getUserFullInfo` response (schema 1.8.67 lines 11501 / 2430 /
        // 2468): `bot_info` carries `description` and a bare
        // `vector<botCommand>` of `commands`.
        let json = r#"{"@type":"userFullInfo","@extra":"7","block_list":null,"bio":{"@type":"formattedText","text":"","entities":[]},"birthdate":null,"bot_info":{"@type":"botInfo","short_description":"A demo bot","description":"This bot demonstrates the info panel.","commands":[{"@type":"botCommand","command":"start","description":"Start the bot","is_ephemeral":false},{"@type":"botCommand","command":"help","description":"Show help","is_ephemeral":false}]}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UserFullInfo {
                bot_info,
                bio,
                photo,
            } => {
                let info = bot_info.expect("bot_info");
                assert_eq!(info.short_description, "A demo bot");
                assert_eq!(info.description, "This bot demonstrates the info panel.");
                assert_eq!(info.commands.len(), 2);
                assert_eq!(info.commands[0].command, "start");
                assert_eq!(info.commands[0].description, "Start the bot");
                assert_eq!(info.commands[1].command, "help");
                assert!(bio.is_empty());
                assert!(photo.is_none());
            }
            other => panic!("{other:?}"),
        }
        // Non-bot full info: `bot_info` null → None.
        let env = parse_envelope(
            r#"{"@type":"userFullInfo","@extra":"8","block_list":null,"bot_info":null}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UserFullInfo { bot_info, .. } => assert!(bot_info.is_none()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn update_user_full_info_parsed() {
        // `updateUserFullInfo` (schema 1.8.67 line 10744): user id explicit,
        // `bot_info` nested under `user_full_info`.
        let json = r#"{"@type":"updateUserFullInfo","user_id":21,"user_full_info":{"@type":"userFullInfo","bot_info":{"@type":"botInfo","short_description":"","description":"Refreshed description.","commands":[{"@type":"botCommand","command":"ping","description":"","is_ephemeral":false}]}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateUserFullInfo {
                user_id, bot_info, ..
            } => {
                assert_eq!(user_id.0, 21);
                let info = bot_info.expect("bot_info");
                assert_eq!(info.description, "Refreshed description.");
                assert_eq!(info.commands.len(), 1);
                assert_eq!(info.commands[0].command, "ping");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn bot_commands_parsed_from_get_commands_response() {
        // `botCommands` (schema 1.8.67 line 829): the `getCommands`
        // response — `bot_user_id:int53` plus a bare
        // `vector<botCommand>`, parsed with the same `parse_bot_command`
        // as `botInfo.commands`.
        let json = r#"{"@type":"botCommands","@extra":"9","bot_user_id":21,"commands":[{"@type":"botCommand","command":"settings","description":"Tweak the bot","is_ephemeral":false},{"@type":"botCommand","command":"help","description":"","is_ephemeral":false}]}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::BotCommands {
                bot_user_id,
                commands,
            } => {
                assert_eq!(bot_user_id.0, 21);
                assert_eq!(commands.len(), 2);
                assert_eq!(commands[0].command, "settings");
                assert_eq!(commands[0].description, "Tweak the bot");
                assert_eq!(commands[1].command, "help");
                assert_eq!(commands[1].description, "");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn bot_commands_parsed_without_command_list() {
        // Missing/null `commands` degrades to an empty list rather than a
        // parse failure — the menu then simply shows no global rows.
        let json = r#"{"@type":"botCommands","@extra":"9","bot_user_id":21}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::BotCommands {
                bot_user_id,
                commands,
            } => {
                assert_eq!(bot_user_id.0, 21);
                assert!(commands.is_empty());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn bot_commands_rejects_missing_bot_user_id() {
        // `bot_user_id:int53` is a required schema field (line 829) —
        // a response without it is a parse error, not a cache entry
        // under user id 0.
        let json = r#"{"@type":"botCommands","@extra":"9","commands":[]}"#;
        assert!(parse_envelope(json).is_err());
    }

    #[test]
    fn inline_keyboard_parsed_from_reply_markup() {
        // `replyMarkupInlineKeyboard` (schema 1.8.67 line 3855):
        // `rows` is a vector of rows; `inlineKeyboardButton` (line 3828)
        // carries `text`, `style:ButtonStyle`, `type`.
        let json = r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Open","icon_custom_emoji_id":0,"style":{"@type":"buttonStylePrimary"},"type":{"@type":"inlineKeyboardButtonTypeUrl","url":"https://example.com"}},{"@type":"inlineKeyboardButton","text":"Tap me","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeCallback","data":"AQID"}}],[{"@type":"inlineKeyboardButton","text":"Search","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleLink"},"type":{"@type":"inlineKeyboardButtonTypeSwitchInline","query":"pic","target_chat":{"@type":"targetChatCurrent"}}}]],"force_reply":false},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Pick one","entities":[]}}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let keyboard = message.reply_markup.expect("reply_markup");
                assert!(!keyboard.force_reply);
                assert_eq!(keyboard.rows.len(), 2);
                assert_eq!(keyboard.rows[0].len(), 2);
                let open = &keyboard.rows[0][0];
                assert_eq!(open.text, "Open");
                assert_eq!(open.style, InlineKeyboardButtonStyle::Primary);
                assert_eq!(
                    open.kind,
                    InlineKeyboardButtonType::Url {
                        url: "https://example.com".to_string()
                    }
                );
                let tap = &keyboard.rows[0][1];
                assert_eq!(tap.style, InlineKeyboardButtonStyle::Default);
                assert_eq!(
                    tap.kind,
                    InlineKeyboardButtonType::Callback {
                        data: vec![1, 2, 3]
                    }
                );
                let search = &keyboard.rows[1][0];
                assert_eq!(search.style, InlineKeyboardButtonStyle::Link);
                assert_eq!(
                    search.kind,
                    InlineKeyboardButtonType::SwitchInline {
                        query: "pic".to_string(),
                        target: InlineKeyboardTargetChat::Current,
                    }
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn inline_keyboard_tolerates_unknown_types() {
        // Unknown button `@type`, unknown `style`, missing `type`, and a
        // non-keyboard `reply_markup` must never crash the parse; unknown
        // buttons become `Unknown` (rendered disabled) and other markups are
        // ignored.
        let json = r#"{"@type":"updateNewMessage","message":{"id":302,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Mystery","style":{"@type":"buttonStyleFuture"},"type":{"@type":"inlineKeyboardButtonTypeQuantum"}}],[{"@type":"inlineKeyboardButton","text":"No type here"}],"not an array"],"force_reply":true},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"x","entities":[]}}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let keyboard = message.reply_markup.expect("reply_markup");
                assert!(keyboard.force_reply);
                // The `"not an array"` row is skipped; tolerance never crashes.
                assert_eq!(keyboard.rows.len(), 2);
                let mystery = &keyboard.rows[0][0];
                assert_eq!(mystery.style, InlineKeyboardButtonStyle::Default);
                assert_eq!(
                    mystery.kind,
                    InlineKeyboardButtonType::Unknown {
                        type_name: "inlineKeyboardButtonTypeQuantum".to_string()
                    }
                );
                assert!(matches!(
                    keyboard.rows[1][0].kind,
                    InlineKeyboardButtonType::Unknown { .. }
                ));
            }
            other => panic!("{other:?}"),
        }
        let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":303,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupShowKeyboard","rows":[],"is_persistent":false,"resize_keyboard":false,"one_time":false,"is_personal":false,"force_reply":false,"input_field_placeholder":""},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"x","entities":[]}}}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                assert!(message.reply_markup.is_none());
            }
            other => panic!("{other:?}"),
        }
        // Absent / null `reply_markup` → None.
        let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":304,"chat_id":21,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"x","entities":[]}}}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                assert!(message.reply_markup.is_none());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn callback_query_answer_parsed() {
        // `getCallbackQueryAnswer` response (schema 1.8.67 line 7747).
        let env = parse_envelope(
            r#"{"@type":"callbackQueryAnswer","@extra":"9","text":"Done!","show_alert":false,"url":""}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::CallbackQueryAnswer(answer) => {
                assert_eq!(answer.text, "Done!");
                assert!(!answer.show_alert);
                assert!(answer.url.is_empty());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn draft_message_text_and_same_chat_reply() {
        let json = r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0,"draft_message":{"@type":"draftMessage","reply_to":{"@type":"inputMessageReplyToMessage","message_id":101,"quote":null,"checklist_task_id":0,"poll_option_id":""},"date":1700000000,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"meet at 6","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewChat { draft, .. } => {
                let draft = draft.expect("draft");
                assert_eq!(draft.text, "meet at 6");
                assert_eq!(draft.reply_to_message_id, Some(MessageId(101)));
            }
            other => panic!("{other:?}"),
        }
        let update = parse_envelope(
            r#"{"@type":"updateChatDraftMessage","chat_id":11,"draft_message":null,"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}]}"#,
        )
        .unwrap();
        match update.payload {
            EnvelopePayload::UpdateChatDraftMessage {
                draft, positions, ..
            } => {
                assert!(draft.is_none());
                assert_eq!(positions.len(), 1);
                assert_eq!(positions[0].order, 9);
            }
            other => panic!("{other:?}"),
        }
        let external = parse_envelope(
            r#"{"@type":"updateChatDraftMessage","chat_id":11,"draft_message":{"@type":"draftMessage","reply_to":{"@type":"inputMessageReplyToExternalMessage","chat_id":12,"message_id":4,"quote":null,"checklist_task_id":0,"poll_option_id":""},"date":1,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"hi","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null},"positions":[]}"#,
        )
        .unwrap();
        match external.payload {
            EnvelopePayload::UpdateChatDraftMessage { draft, .. } => {
                let draft = draft.expect("text kept");
                assert_eq!(draft.text, "hi");
                assert_eq!(draft.reply_to_message_id, None);
            }
            other => panic!("{other:?}"),
        }
        let bot = parse_envelope(
            r#"{"@type":"updateUser","user":{"id":11,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":true,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
        )
        .unwrap();
        match bot.payload {
            EnvelopePayload::UpdateUser { user_id, user } => {
                assert_eq!(user_id, UserId(11));
                assert!(user.is_bot);
                assert_eq!(user.first_name, "Bot");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn new_chat_carries_read_cursors() {
        let json = r#"{"@type":"updateNewChat","chat":{"id":9,"title":"n","type":{"@type":"chatTypePrivate","user_id":9},"unread_count":2,"last_read_inbox_message_id":10,"last_read_outbox_message_id":11}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewChat {
                unread_count,
                last_read_inbox_message_id,
                last_read_outbox_message_id,
                ..
            } => {
                assert_eq!(unread_count, 2);
                assert_eq!(last_read_inbox_message_id.0, 10);
                assert_eq!(last_read_outbox_message_id.0, 11);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn chats_and_found_messages_are_typed() {
        let chats = parse_envelope(
            r#"{"@type":"chats","@extra":"4","total_count":2,"chat_ids":[11,"12"]}"#,
        )
        .unwrap();
        match chats.payload {
            EnvelopePayload::Chats {
                total_count,
                chat_ids,
            } => {
                assert_eq!(total_count, 2);
                assert_eq!(chat_ids, vec![ChatId(11), ChatId(12)]);
            }
            other => panic!("{other:?}"),
        }
        let found = parse_envelope(
            r#"{"@type":"foundMessages","@extra":"5","total_count":1,"next_offset":"n1","messages":[{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_FOUND_hi","entities":[]}}}]}"#,
        )
        .unwrap();
        match found.payload {
            EnvelopePayload::FoundMessages {
                total_count,
                messages,
                next_offset,
            } => {
                assert_eq!(total_count, 1);
                assert_eq!(next_offset, "n1");
                assert_eq!(messages.len(), 1);
                assert_eq!(messages[0].id.0, 101);
                assert_eq!(messages[0].chat_id.0, 11);
                assert_eq!(messages[0].content.preview(), "CANARY_FOUND_hi");
            }
            other => panic!("{other:?}"),
        }
        let schema = include_str!("../../schema/td_api.tl");
        assert!(schema.lines().any(|l| l.starts_with("searchChats ")));
        assert!(schema.lines().any(|l| l.starts_with("searchMessages ")));
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("searchRecentlyFoundChats "))
        );
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("addRecentlyFoundChat "))
        );
        assert!(schema.lines().any(|l| l.starts_with("chats ")));
        assert!(schema.lines().any(|l| l.starts_with("foundMessages ")));
        let in_chat = parse_envelope(
            r#"{"@type":"foundChatMessages","@extra":"6","total_count":2,"next_from_message_id":"40","messages":[{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_CHAT_FOUND","entities":[]}}}]}"#,
        )
        .unwrap();
        match in_chat.payload {
            EnvelopePayload::FoundChatMessages {
                total_count,
                messages,
                next_from_message_id,
            } => {
                assert_eq!(total_count, 2);
                assert_eq!(next_from_message_id.0, 40);
                assert_eq!(messages.len(), 1);
                assert_eq!(messages[0].id.0, 101);
                assert_eq!(messages[0].content.preview(), "CANARY_CHAT_FOUND");
            }
            other => panic!("{other:?}"),
        }
        assert!(schema.lines().any(|l| l.starts_with("searchChatMessages ")));
        assert!(schema.lines().any(|l| l.starts_with("foundChatMessages ")));
    }

    #[test]
    fn view_messages_schema_matches_1_8_67() {
        let schema = include_str!("../../schema/td_api.tl");
        let view = schema
            .lines()
            .find(|l| l.starts_with("viewMessages "))
            .expect("viewMessages");
        assert!(view.contains("message_ids:vector<int53>"));
        assert!(view.contains("source:MessageSource"));
        assert!(view.contains("force_read:Bool"));
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("updateChatReadOutbox "))
        );
        assert!(schema.lines().any(|l| l.starts_with("openChat ")));
        assert!(schema.lines().any(|l| l.starts_with("closeChat ")));
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("messageSourceChatHistory"))
        );
    }

    #[test]
    fn last_message_positions_are_typed() {
        let json = r#"{"@type":"updateChatLastMessage","chat_id":3,"last_message":{"id":1,"chat_id":3,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"preview","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"5","is_pinned":true}]}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateChatLastMessage {
                chat_id,
                last_message,
                positions,
            } => {
                assert_eq!(chat_id.0, 3);
                assert_eq!(
                    last_message.unwrap().content,
                    MessageContent::Text("preview".into())
                );
                assert_eq!(positions.len(), 1);
                assert_eq!(positions[0].order, 5);
                assert!(positions[0].is_pinned);
                assert_eq!(positions[0].list, ChatList::Main);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn message_text_parses_link_entities_and_article_preview() {
        let text = "see https://example.com and the notes";
        let url_at = text.find("https").unwrap();
        let notes_at = text.find("notes").unwrap();
        let thumb = local_file_json(7, "", false, true);
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":8,"chat_id":4,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text_json},"entities":[{{"@type":"textEntity","offset":{url_at},"length":19,"type":{{"@type":"textEntityTypeUrl"}}}},{{"@type":"textEntity","offset":{notes_at},"length":5,"type":{{"@type":"textEntityTypeTextUrl","url":"https://example.com/notes"}}}},{{"@type":"textEntity","offset":0,"length":3,"type":{{"@type":"textEntityTypeBold"}}}}]}},"link_preview":{{"@type":"linkPreview","url":"https://example.com/story","display_url":"example.com","site_name":"Example","title":"A short story","description":{{"@type":"formattedText","text":"Preview body","entities":[]}},"author":"","type":{{"@type":"linkPreviewTypeArticle","photo":{{"@type":"photo","has_stickers":false,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":90,"height":90,"progressive_sizes":[]}}]}}}},"has_large_media":false,"show_large_media":false,"show_media_above_description":false,"skip_confirmation":true,"show_above_text":false,"instant_view_version":0}},"link_preview_options":null}}}}}}"#,
            text_json = serde_json::to_string(text).unwrap(),
        );
        let env = parse_envelope(&json).unwrap();
        let EnvelopePayload::UpdateNewMessage(message) = env.payload else {
            panic!("expected message");
        };
        let MessageContent::Text(content) = message.content else {
            panic!("expected text");
        };
        assert_eq!(content.text, text);
        assert_eq!(content.entities.len(), 3);
        assert!(matches!(
            content.entities[0].kind,
            crate::text::TextEntityKind::Url
        ));
        assert_eq!(
            content.entities[1].open_href(&content.text),
            Some("https://example.com/notes")
        );
        // Phase 4.1: style entities parse alongside links.
        assert!(matches!(
            content.entities[2].kind,
            crate::text::TextEntityKind::Bold
        ));
        assert_eq!(content.entities[2].utf8_start, 0);
        assert_eq!(content.entities[2].utf8_end, 3);
        let preview = content.link_preview.expect("preview");
        assert_eq!(preview.site_name, "Example");
        assert_eq!(preview.title, "A short story");
        assert_eq!(preview.description, "Preview body");
        assert_eq!(preview.url, "https://example.com/story");
        assert!(!preview.show_large_media);
        assert!(!preview.show_above_text);
        let photo = preview.photo.expect("article photo");
        assert_eq!(photo.thumb_size().map(|size| size.file_id), Some(FileId(7)));
        assert_eq!(message.files.len(), 1);
        assert_eq!(message.files[0].id, FileId(7));
    }

    fn local_file_json(id: i32, path: &str, completed: bool, can_download: bool) -> String {
        format!(
            r#"{{"@type":"file","id":{id},"size":12,"expected_size":12,"local":{{"@type":"localFile","path":{path},"can_be_downloaded":{can_download},"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":{completed},"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"CANARY_REMOTE_ID","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":12}}}}"#,
            path = serde_json::to_string(path).unwrap(),
            can_download = can_download,
            completed = completed,
        )
    }

    #[test]
    fn message_photo_parses_sizes_caption_and_flags() {
        let thumb = local_file_json(1, "", false, true);
        let full = local_file_json(2, "/tmp/quill-photo.jpg", true, true);
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":9,"chat_id":4,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":320,"height":240,"progressive_sizes":[]}},{{"@type":"photoSize","type":"x","photo":{full},"width":800,"height":600,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"CANARY_PHOTO_caption","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
        );
        let env = parse_envelope(&json).unwrap();
        match &env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let MessageContent::Photo(photo) = &message.content else {
                    panic!("{:?}", message.content);
                };
                assert_eq!(photo.caption, "CANARY_PHOTO_caption");
                assert!(!photo.is_secret);
                assert!(!photo.has_spoiler);
                assert_eq!(photo.sizes.len(), 2);
                assert_eq!(photo.thumb_size().unwrap().type_name, "m");
                assert_eq!(photo.thumb_size().unwrap().file_id.0, 1);
                assert_eq!(photo.largest_size().unwrap().file_id.0, 2);
                assert_eq!(photo.open_file_id().unwrap().0, 2);
                assert_eq!(message.files.len(), 2);
                assert_eq!(message.files[0].id.0, 1);
                assert!(message.files[0].needs_download());
                assert_eq!(message.files[1].usable_path(), Some("/tmp/quill-photo.jpg"));
            }
            other => panic!("{other:?}"),
        }
        let debug = format!("{env:?}");
        assert!(!debug.contains("CANARY_REMOTE_ID"));
    }

    #[test]
    fn message_document_parses_name_mime_and_file() {
        let file = local_file_json(8, "", false, true);
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":3,"chat_id":4,"is_outgoing":false,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"notes.txt","mime_type":"text/plain","document":{file}}},"caption":{{"@type":"formattedText","text":"CANARY_DOC_caption","entities":[]}}}}}}}}"#
        );
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let MessageContent::Document(doc) = message.content else {
                    panic!("{:?}", message.content);
                };
                assert_eq!(doc.file_name, "notes.txt");
                assert_eq!(doc.mime_type, "text/plain");
                assert_eq!(doc.caption, "CANARY_DOC_caption");
                assert_eq!(doc.file_id.0, 8);
                assert_eq!(message.files[0].id.0, 8);
                assert!(message.files[0].needs_download());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn update_file_and_file_response_are_typed() {
        let file = local_file_json(4, "/tmp/done.bin", true, true);
        let update = parse_envelope(&format!(r#"{{"@type":"updateFile","file":{file}}}"#)).unwrap();
        match update.payload {
            EnvelopePayload::UpdateFile(parsed) => {
                assert_eq!(parsed.id.0, 4);
                assert_eq!(parsed.usable_path(), Some("/tmp/done.bin"));
            }
            other => panic!("{other:?}"),
        }
        let response = parse_envelope(
            r#"{"@type":"file","@extra":"12","id":4,"size":12,"expected_size":12,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":true,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":12}}"#,
        )
        .unwrap();
        match response.payload {
            EnvelopePayload::File(parsed) => {
                assert_eq!(parsed.id.0, 4);
                assert!(parsed.local.is_downloading_active);
                assert!(parsed.needs_download());
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(response.extra, Some(crate::ids::RequestId(12)));
    }

    #[test]
    fn media_schema_matches_1_8_67() {
        let schema = include_str!("../../schema/td_api.tl");
        let photo = schema
            .lines()
            .find(|l| l.starts_with("messagePhoto "))
            .expect("messagePhoto");
        assert!(photo.contains("photo:photo"));
        assert!(photo.contains("caption:formattedText"));
        assert!(photo.contains("is_secret:Bool"));
        assert!(photo.contains("has_spoiler:Bool"));
        let document = schema
            .lines()
            .find(|l| l.starts_with("messageDocument "))
            .expect("messageDocument");
        assert!(document.contains("document:document"));
        assert!(document.contains("caption:formattedText"));
        let download = schema
            .lines()
            .find(|l| l.starts_with("downloadFile "))
            .expect("downloadFile");
        assert!(download.contains("file_id:int32"));
        assert!(download.contains("priority:int32"));
        assert!(download.contains("offset:int53"));
        assert!(download.contains("limit:int53"));
        assert!(download.contains("synchronous:Bool"));
        assert!(schema.lines().any(|l| l.starts_with("updateFile ")));
        assert!(schema.lines().any(|l| l.starts_with("localFile ")));
        assert!(schema.lines().any(|l| l.starts_with("photoSize ")));
    }

    #[test]
    fn message_sticker_keeps_webp_thumb_and_file() {
        let sticker_file = local_file_json(41, "", false, true);
        let thumb = local_file_json(42, "/tmp/sticker.webp", true, true);
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":8,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageSticker","is_premium":false,"sticker":{{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatWebp"}},"width":128,"height":128,"file":{thumb}}},"sticker":{sticker_file}}}}}}}}}"#
        );
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let MessageContent::Sticker(sticker) = &message.content else {
                    panic!("{:?}", message.content);
                };
                assert_eq!(sticker.emoji, "😀");
                assert_eq!(sticker.file_id, FileId(41));
                assert_eq!(sticker.thumb_file_id, Some(FileId(42)));
                assert_eq!(sticker.display_file_id(), Some(FileId(42)));
                assert_eq!(sticker.format, StickerFormat::Webp);
                assert!(message.files.iter().any(|file| file.id == FileId(42)));
                assert_eq!(message.content.preview(), "😀");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn sticker_sets_and_sticker_set_parse_1_8_67() {
        let sets = parse_envelope(
            r#"{"@type":"stickerSets","total_count":1,"sets":[{"@type":"stickerSetInfo","id":"77","title":"Demo","name":"DemoStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":true,"sticker_type":{"@type":"stickerTypeRegular"},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"size":2,"covers":[]}]}"#,
        )
        .unwrap();
        match sets.payload {
            EnvelopePayload::StickerSets { total_count, sets } => {
                assert_eq!(total_count, 1);
                assert_eq!(sets[0].id, 77);
                assert!(sets[0].is_installed);
                assert!(sets[0].is_official);
                assert_eq!(sets[0].title, "Demo");
            }
            other => panic!("{other:?}"),
        }
        let file = local_file_json(41, "/tmp/s.webp", true, true);
        let set = parse_envelope(&format!(
            r#"{{"@type":"stickerSet","id":"77","title":"Demo","name":"DemoStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":true,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"stickers":[{{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatTgs"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":{file}}}],"emojis":[]}}"#
        ))
        .unwrap();
        match set.payload {
            EnvelopePayload::StickerSet { id, stickers, .. } => {
                assert_eq!(id, 77);
                assert_eq!(stickers[0].format, StickerFormat::Tgs);
                assert_eq!(stickers[0].file_id, FileId(41));
                assert!(stickers[0].thumb_file_id.is_none());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn message_video_parses_1_8_67_fields() {
        let clip = local_file_json(33, "", false, true);
        let thumb = local_file_json(42, "/tmp/video-thumb.jpg", true, true);
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":9,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":42,"width":640,"height":360,"file_name":"clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":120,"height":68,"file":{thumb}}},"video":{clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":3,"caption":{{"@type":"formattedText","text":"see this","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
        );
        let env = parse_envelope(&json).unwrap();
        let EnvelopePayload::UpdateNewMessage(message) = env.payload else {
            panic!("expected message");
        };
        let MessageContent::Video(video) = &message.content else {
            panic!("{:?}", message.content);
        };
        assert_eq!(video.duration, 42);
        assert_eq!(video.width, 640);
        assert_eq!(video.height, 360);
        assert_eq!(video.file_name, "clip.mp4");
        assert_eq!(video.mime_type, "video/mp4");
        assert!(video.supports_streaming);
        assert!(!video.has_stickers);
        assert_eq!(video.start_timestamp, 3);
        assert_eq!(video.file_id, FileId(33));
        assert_eq!(video.play_file_id(), Some(FileId(33)));
        assert_eq!(video.thumb_file_id(), Some(FileId(42)));
        assert_eq!(video.thumb_width, 120);
        assert_eq!(video.thumb_height, 68);
        assert_eq!(video.caption, "see this");
        assert_eq!(message.content.preview(), "see this");
        assert!(message.files.iter().any(|file| file.id == FileId(33)));
        assert!(message.files.iter().any(|file| file.id == FileId(42)));
    }

    #[test]
    fn message_video_note_parses_1_8_67_fields() {
        let waveform = base64::engine::general_purpose::STANDARD.encode([0xF8, 0x02]);
        let clip = local_file_json(33, "", false, true);
        let thumb = local_file_json(42, "/tmp/note-thumb.jpg", true, true);
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":9,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":8,"waveform":"{waveform}","length":240,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":120,"height":120,"file":{thumb}}},"speech_recognition_result":null,"video":{clip}}},"is_viewed":false,"is_secret":false}}}}}}"#
        );
        let env = parse_envelope(&json).unwrap();
        let EnvelopePayload::UpdateNewMessage(message) = env.payload else {
            panic!("expected message");
        };
        let MessageContent::VideoNote(note) = &message.content else {
            panic!("{:?}", message.content);
        };
        assert_eq!(note.duration, 8);
        assert_eq!(note.length, 240);
        assert_eq!(note.waveform, vec![0xF8, 0x02]);
        assert!(!note.is_viewed);
        assert!(!note.is_secret);
        assert_eq!(note.file_id, FileId(33));
        assert_eq!(note.play_file_id(), Some(FileId(33)));
        assert_eq!(note.thumb_file_id(), Some(FileId(42)));
        assert_eq!(note.thumb_width, 120);
        assert_eq!(note.thumb_height, 120);
        assert_eq!(message.content.preview(), "Video note");
        assert!(message.files.iter().any(|file| file.id == FileId(33)));
        assert!(message.files.iter().any(|file| file.id == FileId(42)));
    }

    #[test]
    fn message_animation_and_saved_list_parse_1_8_67() {
        let clip = local_file_json(33, "", false, true);
        let thumb = local_file_json(42, "/tmp/gif-thumb.jpg", true, true);
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":8,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageAnimation","animation":{{"@type":"animation","duration":2,"width":240,"height":140,"file_name":"wave.mp4","mime_type":"video/mp4","has_stickers":false,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":120,"height":70,"file":{thumb}}},"animation":{clip}}},"caption":{{"@type":"formattedText","text":"loop","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
        );
        let env = parse_envelope(&json).unwrap();
        let EnvelopePayload::UpdateNewMessage(message) = env.payload else {
            panic!("expected message");
        };
        let MessageContent::Animation(animation) = &message.content else {
            panic!("{:?}", message.content);
        };
        assert_eq!(animation.mime_type, "video/mp4");
        assert_eq!(animation.file_id, FileId(33));
        assert_eq!(animation.thumb_file_id(), Some(FileId(42)));
        assert_eq!(animation.caption, "loop");
        assert_eq!(message.content.preview(), "loop");
        let saved = parse_envelope(
            r#"{"@type":"animations","animations":[{"@type":"animation","duration":1,"width":10,"height":10,"file_name":"a.mp4","mime_type":"video/mp4","has_stickers":false,"minithumbnail":null,"thumbnail":null,"animation":{"@type":"file","id":33,"size":1,"expected_size":1,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":1}}}]}"#,
        )
        .unwrap();
        match saved.payload {
            EnvelopePayload::Animations { animations, .. } => {
                assert_eq!(animations.len(), 1);
                assert_eq!(animations[0].file_id, FileId(33));
                assert!(animations[0].thumb_file_id.is_none());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn secret_photo_is_flagged() {
        let file = local_file_json(1, "", false, true);
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":1,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":true,"is_secret":true}}}}}}"#
        );
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let MessageContent::Photo(photo) = message.content else {
                    panic!("{:?}", message.content);
                };
                assert!(photo.is_secret);
                assert!(photo.has_spoiler);
                assert!(!photo.click_requests_download());
                assert_eq!(
                    photo.placeholder_label(false, false),
                    "Secret photo — not downloaded"
                );
                assert_eq!(
                    photo.placeholder_label(true, false),
                    "Secret photo — downloading…"
                );
                assert_eq!(photo.placeholder_label(false, true), "Secret photo — ready");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn spoiler_placeholder_follows_file_state_and_may_download() {
        let file = local_file_json(1, "", false, true);
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":1,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":true,"is_secret":false}}}}}}"#
        );
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let MessageContent::Photo(photo) = message.content else {
                    panic!("{:?}", message.content);
                };
                assert!(photo.click_requests_download());
                assert_eq!(
                    photo.placeholder_label(false, false),
                    "Photo (spoiler) — not downloaded"
                );
                assert_eq!(
                    photo.placeholder_label(true, false),
                    "Photo (spoiler) — downloading…"
                );
                assert_eq!(
                    photo.placeholder_label(false, true),
                    "Photo (spoiler) — ready"
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn message_reply_to_message_is_typed() {
        let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":104,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"sounds good","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":11,"message_id":101,"quote":{"@type":"textQuote","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]},"position":0,"is_manual":false},"checklist_task_id":0,"poll_option_id":""}}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let reply = message.reply_to.expect("reply_to");
                assert_eq!(reply.chat_id.0, 11);
                assert_eq!(reply.message_id.0, 101);
                assert_eq!(
                    reply.quote_text.as_deref(),
                    Some("Hello from injected JSON.")
                );
                assert!(reply.is_same_chat(ChatId(11)));
            }
            other => panic!("{other:?}"),
        }
        let story = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":2,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"story","entities":[]}},"reply_to":{"@type":"messageReplyToStory","story_poster_chat_id":11,"story_id":3}}}"#,
        )
        .unwrap();
        match story.payload {
            EnvelopePayload::UpdateNewMessage(message) => assert_eq!(message.reply_to, None),
            other => panic!("{other:?}"),
        }
        let schema = include_str!("../../schema/td_api.tl");
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("messageReplyToMessage "))
        );
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("inputMessageReplyToMessage "))
        );
        assert!(schema.lines().any(|l| l.starts_with("textQuote ")));
        assert!(schema.lines().any(|l| l.starts_with("inputTextQuote ")));
    }

    #[test]
    fn update_message_content_is_typed() {
        let env = parse_envelope(
            r#"{"@type":"updateMessageContent","chat_id":11,"message_id":102,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_EDITED","entities":[]}}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateMessageContent {
                chat_id,
                message_id,
                content,
                files,
            } => {
                assert_eq!(chat_id.0, 11);
                assert_eq!(message_id.0, 102);
                assert_eq!(content, MessageContent::Text("CANARY_EDITED".into()));
                assert!(files.is_empty());
            }
            other => panic!("{other:?}"),
        }
        let schema = include_str!("../../schema/td_api.tl");
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("updateMessageContent "))
        );
        assert!(schema.lines().any(|l| l.starts_with("editMessageText ")));
        assert!(schema.lines().any(|l| l.starts_with("editMessageCaption ")));
        assert!(schema.lines().any(|l| l.starts_with("deleteMessages ")));
    }

    #[test]
    fn message_forward_info_and_messages_are_typed() {
        let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":105,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"fwd body","entities":[]}},"forward_info":{"@type":"messageForwardInfo","origin":{"@type":"messageOriginHiddenUser","sender_name":"Ada Lovelace"},"date":1700000000,"source":null,"public_service_announcement_type":""}}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let info = message.forward_info.expect("forward_info");
                assert_eq!(
                    info.origin,
                    MessageOrigin::HiddenUser {
                        sender_name: "Ada Lovelace".into()
                    }
                );
                assert_eq!(info.date, 1_700_000_000);
            }
            other => panic!("{other:?}"),
        }
        let messages = parse_envelope(
            r#"{"@type":"messages","@extra":"34","total_count":2,"messages":[{"id":80,"chat_id":12,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}},"forward_info":{"@type":"messageForwardInfo","origin":{"@type":"messageOriginUser","sender_user_id":11},"date":1}},null]}"#,
        )
        .unwrap();
        match messages.payload {
            EnvelopePayload::Messages(parsed) => {
                assert_eq!(parsed.len(), 1);
                assert_eq!(parsed[0].id.0, 80);
                assert_eq!(parsed[0].chat_id.0, 12);
                assert!(matches!(
                    parsed[0].forward_info.as_ref().map(|i| &i.origin),
                    Some(MessageOrigin::User { user_id }) if user_id.0 == 11
                ));
            }
            other => panic!("{other:?}"),
        }
        let schema = include_str!("../../schema/td_api.tl");
        assert!(schema.lines().any(|l| l.starts_with("forwardMessages ")));
        assert!(schema.lines().any(|l| l.starts_with("messages ")));
        assert!(schema.lines().any(|l| l.starts_with("messageForwardInfo ")));
        assert!(schema.lines().any(|l| l.starts_with("messageOriginUser ")));
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("messageOriginHiddenUser "))
        );
    }

    #[test]
    fn message_interaction_info_and_update_are_typed() {
        let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"react me","entities":[]}},"interaction_info":{"@type":"messageInteractionInfo","view_count":4,"forward_count":1,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"total_count":3,"is_chosen":true,"used_sender_id":null,"recent_sender_ids":[]},{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":2,"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":true}}}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let info = message.interaction_info.expect("interaction_info");
                assert_eq!(info.view_count, 4);
                assert_eq!(info.forward_count, 1);
                let chips = info.emoji_chips();
                assert_eq!(chips.len(), 2);
                assert_eq!(chips[0].chip_label().as_deref(), Some("❤ 3"));
                assert!(chips[0].is_chosen);
                assert_eq!(chips[1].chip_label().as_deref(), Some("👍 2"));
                assert!(!chips[1].is_chosen);
                assert!(info.chosen_emoji("❤"));
                assert!(!info.chosen_emoji("👍"));
            }
            other => panic!("{other:?}"),
        }
        let update = parse_envelope(
            r#"{"@type":"updateMessageInteractionInfo","chat_id":11,"message_id":101,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":1,"is_chosen":true,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}"#,
        )
        .unwrap();
        match update.payload {
            EnvelopePayload::UpdateMessageInteractionInfo {
                chat_id,
                message_id,
                interaction_info,
            } => {
                assert_eq!(chat_id.0, 11);
                assert_eq!(message_id.0, 101);
                let info = interaction_info.expect("interaction_info");
                assert_eq!(info.emoji_chips()[0].chip_label().as_deref(), Some("👍 1"));
                assert!(info.chosen_emoji("👍"));
            }
            other => panic!("{other:?}"),
        }
        let cleared = parse_envelope(
            r#"{"@type":"updateMessageInteractionInfo","chat_id":11,"message_id":101,"interaction_info":null}"#,
        )
        .unwrap();
        match cleared.payload {
            EnvelopePayload::UpdateMessageInteractionInfo {
                interaction_info, ..
            } => assert_eq!(interaction_info, None),
            other => panic!("{other:?}"),
        }
        let after_unreact = toggle_chosen_emoji_reaction(
            Some(&MessageInteractionInfo {
                reactions: Some(MessageReactions {
                    reactions: vec![MessageReaction {
                        reaction_type: ReactionType::emoji("❤"),
                        total_count: 3,
                        is_chosen: true,
                    }],
                    are_tags: false,
                }),
                ..MessageInteractionInfo::default()
            }),
            "❤",
        );
        assert!(!after_unreact.chosen_emoji("❤"));
        assert_eq!(
            after_unreact.emoji_chips()[0].chip_label().as_deref(),
            Some("❤ 2")
        );
        let schema = include_str!("../../schema/td_api.tl");
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("updateMessageInteractionInfo "))
        );
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("messageInteractionInfo "))
        );
        assert!(schema.lines().any(|l| l.starts_with("messageReactions ")));
        assert!(schema.lines().any(|l| l.starts_with("messageReaction ")));
        assert!(schema.lines().any(|l| l.starts_with("reactionTypeEmoji ")));
        assert!(schema.lines().any(|l| l.starts_with("addMessageReaction ")));
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("removeMessageReaction "))
        );
    }

    #[test]
    fn message_is_pinned_and_update_are_typed() {
        let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"is_pinned":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"pinned","entities":[]}}}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                assert!(message.is_pinned);
                assert_eq!(message.id.0, 101);
            }
            other => panic!("{other:?}"),
        }
        let update = parse_envelope(
            r#"{"@type":"updateMessageIsPinned","chat_id":11,"message_id":101,"is_pinned":false}"#,
        )
        .unwrap();
        match update.payload {
            EnvelopePayload::UpdateMessageIsPinned {
                chat_id,
                message_id,
                is_pinned,
            } => {
                assert_eq!(chat_id.0, 11);
                assert_eq!(message_id.0, 101);
                assert!(!is_pinned);
            }
            other => panic!("{other:?}"),
        }
        let schema = include_str!("../../schema/td_api.tl");
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("updateMessageIsPinned "))
        );
        assert!(schema.lines().any(|l| l.starts_with("pinChatMessage ")));
        assert!(schema.lines().any(|l| l.starts_with("unpinChatMessage ")));
    }

    #[test]
    fn message_voice_note_keeps_duration_waveform_and_listened() {
        let waveform = base64::engine::general_purpose::STANDARD.encode([0xF8, 0x02]);
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":8,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageVoiceNote","voice_note":{{"@type":"voiceNote","duration":12,"waveform":"{waveform}","mime_type":"audio/ogg","speech_recognition_result":null,"voice":{{"@type":"file","id":4,"size":9,"expected_size":9,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":9}}}}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"is_listened":false}}}}}}"#
        );
        let env = parse_envelope(&json).unwrap();
        let EnvelopePayload::UpdateNewMessage(message) = env.payload else {
            panic!("voice note");
        };
        let MessageContent::VoiceNote(note) = &message.content else {
            panic!("content");
        };
        assert_eq!(note.duration, 12);
        assert_eq!(note.mime_type, "audio/ogg");
        assert!(!note.is_listened);
        assert_eq!(note.file_id, FileId(4));
        assert_eq!(note.waveform, vec![0xF8, 0x02]);
        assert_eq!(message.content.preview(), "Voice message");
        assert_eq!(message.files[0].id, FileId(4));
        let opened =
            parse_envelope(r#"{"@type":"updateMessageContentOpened","chat_id":11,"message_id":8}"#)
                .unwrap();
        match opened.payload {
            EnvelopePayload::UpdateMessageContentOpened {
                chat_id,
                message_id,
            } => {
                assert_eq!(chat_id.0, 11);
                assert_eq!(message_id.0, 8);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn message_audio_parses_1_8_67_fields() {
        let mini = base64::engine::general_purpose::STANDARD.encode([0xFF, 0xD8, 0xFF]);
        let track = local_file_json(7, "", false, true);
        let cover = local_file_json(8, "/tmp/cover.jpg", true, true);
        let external = local_file_json(9, "", false, true);
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":12,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageAudio","audio":{{"@type":"audio","duration":214,"title":"Night Drive","performer":"Ada","file_name":"night.mp3","mime_type":"audio/mpeg","album_cover_minithumbnail":{{"@type":"minithumbnail","width":8,"height":8,"data":"{mini}"}},"album_cover_thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":90,"height":90,"file":{cover}}},"external_album_covers":[{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":320,"height":320,"file":{external}}}],"audio":{track}}},"caption":{{"@type":"formattedText","text":"from the album","entities":[]}}}}}}}}"#
        );
        let env = parse_envelope(&json).unwrap();
        let EnvelopePayload::UpdateNewMessage(message) = env.payload else {
            panic!("expected message");
        };
        let MessageContent::Audio(audio) = &message.content else {
            panic!("{:?}", message.content);
        };
        assert_eq!(audio.duration, 214);
        assert_eq!(audio.title, "Night Drive");
        assert_eq!(audio.performer, "Ada");
        assert_eq!(audio.file_name, "night.mp3");
        assert_eq!(audio.mime_type, "audio/mpeg");
        assert_eq!(audio.caption, "from the album");
        assert_eq!(audio.file_id, FileId(7));
        assert_eq!(audio.play_file_id(), Some(FileId(7)));
        let mini_thumb = audio.album_cover_minithumbnail.as_ref().expect("mini");
        assert_eq!(mini_thumb.width, 8);
        assert_eq!(mini_thumb.data, vec![0xFF, 0xD8, 0xFF]);
        let cover_thumb = audio.album_cover_thumbnail.as_ref().expect("cover");
        assert_eq!(cover_thumb.file_id, FileId(8));
        assert_eq!(cover_thumb.width, 90);
        assert_eq!(audio.external_album_covers.len(), 1);
        assert_eq!(audio.external_album_covers[0].file_id, FileId(9));
        assert_eq!(audio.cover_file_id(), Some(FileId(8)));
        assert_eq!(message.content.preview(), "from the album");
        assert!(message.files.iter().any(|file| file.id == FileId(7)));
        assert!(message.files.iter().any(|file| file.id == FileId(8)));
        assert!(message.files.iter().any(|file| file.id == FileId(9)));
    }
}

#[cfg(test)]
mod channel_envelope_tests {
    use super::*;

    #[test]
    fn chat_member_status_constructors_parse() {
        for (ctor, expected) in [
            ("chatMemberStatusCreator", ChannelMemberStatus::Creator),
            (
                "chatMemberStatusAdministrator",
                ChannelMemberStatus::Administrator,
            ),
            ("chatMemberStatusMember", ChannelMemberStatus::Member),
            (
                "chatMemberStatusRestricted",
                ChannelMemberStatus::Restricted,
            ),
            ("chatMemberStatusLeft", ChannelMemberStatus::Left),
            ("chatMemberStatusBanned", ChannelMemberStatus::Banned),
        ] {
            let json = format!(
                r#"{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{{"@type":"{ctor}"}}}}"#,
            );
            let env = parse_envelope(&json).unwrap();
            match env.payload {
                EnvelopePayload::ChatMember { member } => {
                    assert_eq!(member.member_id, MessageSender::User { user_id: 777 });
                    assert_eq!(member.status, expected);
                    // Bare status constructors carry no rights block.
                    assert_eq!(member.admin_can_post_messages, None);
                }
                other => panic!("{other:?}"),
            }
        }
        assert!(ChannelMemberStatus::Creator.is_admin());
        assert!(ChannelMemberStatus::Administrator.is_admin());
        assert!(!ChannelMemberStatus::Member.is_admin());
        assert!(ChannelMemberStatus::Member.is_joined());
        assert!(!ChannelMemberStatus::Left.is_joined());
    }

    #[test]
    fn chat_member_administrator_rights_can_post_messages() {
        // `rights.can_post_messages` rides on `chatMemberStatusAdministrator`
        // (schema 1.8.67: `chatMemberStatusAdministrator can_be_edited:Bool
        // rights:chatAdministratorRights`), not on the status itself.
        for (can_post, expected) in [(true, Some(true)), (false, Some(false))] {
            let json = format!(
                r#"{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{{"@type":"chatAdministratorRights","can_post_messages":{can_post}}}}}}}"#,
            );
            let env = parse_envelope(&json).unwrap();
            match env.payload {
                EnvelopePayload::ChatMember { member } => {
                    assert_eq!(member.status, ChannelMemberStatus::Administrator);
                    assert_eq!(member.admin_can_post_messages, expected);
                }
                other => panic!("{other:?}"),
            }
        }
        // Missing rights block: no posting-right claim either way.
        let json = r#"{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":777},"status":{"@type":"chatMemberStatusAdministrator"}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::ChatMember { member } => {
                assert_eq!(member.status, ChannelMemberStatus::Administrator);
                assert_eq!(member.admin_can_post_messages, None);
            }
            other => panic!("{other:?}"),
        }
        // Non-admin statuses never carry the right, even with a rights block.
        let json = r#"{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":777},"status":{"@type":"chatMemberStatusMember","rights":{"@type":"chatAdministratorRights","can_post_messages":true}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::ChatMember { member } => {
                assert_eq!(member.status, ChannelMemberStatus::Member);
                assert_eq!(member.admin_can_post_messages, None);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn update_chat_member_keeps_new_member() {
        let json = r#"{"@type":"updateChatMember","chat_id":13,"actor_user_id":1,"date":1,"invite_link":null,"via_join_request":false,"via_chat_folder_invite_link":false,"old_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":777},"status":{"@type":"chatMemberStatusLeft"}},"new_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":777},"status":{"@type":"chatMemberStatusMember"}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateChatMember { chat_id, member } => {
                assert_eq!(chat_id, ChatId(13));
                assert_eq!(member.status, ChannelMemberStatus::Member);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn join_chat_results_parse_without_invented_variants() {
        let success = parse_envelope(r#"{"@type":"chatJoinResultSuccess","chat_id":13}"#).unwrap();
        match success.payload {
            EnvelopePayload::JoinChatResult(ChatJoinResult::Success { chat_id }) => {
                assert_eq!(chat_id, ChatId(13));
            }
            other => panic!("{other:?}"),
        }
        for (ctor, expected) in [
            ("chatJoinResultRequestSent", ChatJoinResult::RequestSent),
            (
                "chatJoinResultGuardBotApprovalRequired",
                ChatJoinResult::GuardBotApprovalRequired,
            ),
            ("chatJoinResultDeclined", ChatJoinResult::Declined),
        ] {
            let json = format!(r#"{{"@type":"{ctor}"}}"#);
            let env = parse_envelope(&json).unwrap();
            match env.payload {
                EnvelopePayload::JoinChatResult(result) => assert_eq!(result, expected),
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn me_response_keeps_id_only() {
        let env = parse_envelope(r#"{"@type":"user","id":777,"is_bot":false}"#).unwrap();
        match env.payload {
            EnvelopePayload::Me { user_id } => assert_eq!(user_id, 777),
            other => panic!("{other:?}"),
        }
    }

    // Phase 4.2: `messagePoll` (schema 1.8.67 line 5241), `poll` (line 711),
    // `pollOption` (line 456), `pollTypeRegular` (line 468).
    fn message_poll_json(closed: bool) -> String {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":106,"chat_id":15,"is_outgoing":false,"content":{{"@type":"messagePoll","poll":{{"@type":"poll","id":9001,"question":{{"@type":"formattedText","text":"Lunch?","entities":[]}},"options":[{{"@type":"pollOption","id":"a","text":{{"@type":"formattedText","text":"Sushi","entities":[]}},"voter_count":12,"vote_percentage":55,"is_chosen":true}},{{"@type":"pollOption","id":"b","text":{{"@type":"formattedText","text":"Pizza","entities":[]}},"voter_count":7,"vote_percentage":32,"is_chosen":false}}],"total_voter_count":19,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":{closed},"type":{{"@type":"pollTypeRegular"}}}},"description":{{"@type":"formattedText","text":"","entities":[]}},"can_add_option":false}}}}}}"#
        )
    }

    #[test]
    fn message_poll_parses_regular_open_with_chosen_option() {
        let env = parse_envelope(&message_poll_json(false)).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let MessageContent::Poll(poll_content) = &message.content else {
                    panic!("{:?}", message.content);
                };
                let poll = &poll_content.poll;
                assert_eq!(poll.id, 9001);
                assert_eq!(poll.question, "Lunch?");
                assert_eq!(poll.options.len(), 2);
                assert_eq!(poll.options[0].text, "Sushi");
                assert_eq!(poll.options[0].voter_count, 12);
                assert_eq!(poll.options[0].vote_percentage, 55);
                assert!(poll.options[0].is_chosen);
                assert!(!poll.options[1].is_chosen);
                assert_eq!(poll.total_voter_count, 19);
                assert!(poll.is_anonymous);
                assert!(!poll.allows_multiple_answers);
                assert!(poll.allows_revoting);
                assert!(!poll.is_closed);
                assert!(matches!(poll.poll_type, PollType::Regular));
                assert!(poll.can_vote());
                assert_eq!(message.content.preview(), "Lunch?");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn message_poll_parses_quiz_closed() {
        let json = r#"{"@type":"updateNewMessage","message":{"id":107,"chat_id":15,"is_outgoing":false,"content":{"@type":"messagePoll","poll":{"@type":"poll","id":9002,"question":{"@type":"formattedText","text":"Red planet?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Mars","entities":[]},"voter_count":18,"vote_percentage":72,"is_chosen":false}],"total_voter_count":25,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":false,"is_closed":true,"type":{"@type":"pollTypeQuiz","correct_option_ids":[0],"explanation":{"@type":"formattedText","text":"","entities":[]}}},"description":{"@type":"formattedText","text":"","entities":[]},"can_add_option":false}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let MessageContent::Poll(poll_content) = &message.content else {
                    panic!("{:?}", message.content);
                };
                let poll = &poll_content.poll;
                assert!(poll.is_closed);
                assert!(!poll.can_vote());
                match &poll.poll_type {
                    PollType::Quiz { correct_option_ids } => {
                        assert_eq!(correct_option_ids, &[0]);
                    }
                    other => panic!("{other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn update_poll_parses_and_applies_new_counts() {
        let env = parse_envelope(
            r#"{"@type":"updatePoll","poll":{"@type":"poll","id":9001,"question":{"@type":"formattedText","text":"Lunch?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Sushi","entities":[]},"voter_count":13,"vote_percentage":56,"is_chosen":true}],"total_voter_count":23,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"type":{"@type":"pollTypeRegular"}}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdatePoll { poll } => {
                assert_eq!(poll.id, 9001);
                assert_eq!(poll.total_voter_count, 23);
                assert_eq!(poll.options[0].voter_count, 13);
                assert_eq!(poll.options[0].vote_percentage, 56);
                assert!(poll.options[0].is_chosen);
            }
            other => panic!("{other:?}"),
        }
    }

    // Phase 4.3: `messageLocation` (schema 1.8.67 line 5214), `location`
    // (line 646).
    #[test]
    fn message_location_parses_coordinates() {
        let json = r#"{"@type":"updateNewMessage","message":{"id":108,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageLocation","location":{"@type":"location","latitude":37.7749,"longitude":-122.4194,"horizontal_accuracy":15.6}}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let MessageContent::Location(content) = &message.content else {
                    panic!("{:?}", message.content);
                };
                assert!(content.live.is_none());
                assert_eq!(content.location.lat_e6, 37_774_900);
                assert_eq!(content.location.lon_e6, -122_419_400);
                assert_eq!(content.location.accuracy_m, 16);
                assert_eq!(content.location.coords_label(), "37.7749, -122.4194");
                assert_eq!(
                    content.location.open_street_map_url(),
                    "https://www.openstreetmap.org/?mlat=37.774900&mlon=-122.419400"
                );
                assert_eq!(message.content.preview(), "📍 Location");
            }
            other => panic!("{other:?}"),
        }
    }

    // Phase 4.3: `messageLiveLocation` (schema 1.8.67 line 5211) +
    // `liveLocation` (line 653). The task's live fields live here, not on
    // `messageLocation`.
    #[test]
    fn message_live_location_parses_live_fields() {
        let json = r#"{"@type":"updateNewMessage","message":{"id":109,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageLiveLocation","location":{"@type":"liveLocation","location":{"@type":"location","latitude":48.8566,"longitude":2.3522,"horizontal_accuracy":0},"live_period":900,"heading":90,"proximity_alert_radius":500},"expires_in":600}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let MessageContent::Location(content) = &message.content else {
                    panic!("{:?}", message.content);
                };
                let live = content.live.expect("live state");
                assert_eq!(live.live_period, 900);
                assert_eq!(live.expires_in, 600);
                assert_eq!(live.heading, 90);
                assert_eq!(live.proximity_alert_radius, 500);
                assert_eq!(content.location.lat_e6, 48_856_600);
                assert_eq!(content.location.lon_e6, 2_352_200);
                assert_eq!(content.location.accuracy_m, 0);
                assert_eq!(
                    live.status_label(),
                    "Live · expires in 10:00 · heading 90° · proximity alert ≤ 500 m"
                );
                assert_eq!(message.content.preview(), "📍 Live location");
            }
            other => panic!("{other:?}"),
        }
    }

    // Phase 4.3: `messageVenue` (schema 1.8.67 line 5217), `venue` (line
    // 663). Provider `id` / `type` are dropped by design.
    #[test]
    fn message_venue_parses_all_fields() {
        let json = r#"{"@type":"updateNewMessage","message":{"id":110,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageVenue","venue":{"@type":"venue","location":{"@type":"location","latitude":37.7955,"longitude":-122.3937,"horizontal_accuracy":0},"title":"Ferry Building","address":"1 Ferry Building, San Francisco","provider":"foursquare","id":"4a1a2b3c","type":"Food"}}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let MessageContent::Venue(venue) = &message.content else {
                    panic!("{:?}", message.content);
                };
                assert_eq!(venue.title, "Ferry Building");
                assert_eq!(venue.address, "1 Ferry Building, San Francisco");
                assert_eq!(venue.provider, "foursquare");
                assert_eq!(venue.location.coords_label(), "37.7955, -122.3937");
                assert_eq!(message.content.preview(), "📍 Ferry Building");
            }
            other => panic!("{other:?}"),
        }
    }

    // Phase 4.3: `messageContact` (schema 1.8.67 line 5220), `contact`
    // (line 640). The vCard parses without breaking; user_id 0 is
    // unknown.
    #[test]
    fn message_contact_parses_with_vcard() {
        let json = r#"{"@type":"updateNewMessage","message":{"id":111,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageContact","contact":{"@type":"contact","phone_number":"+14155550123","first_name":"Ada","last_name":"Lovelace","vcard":"BEGIN:VCARD\nFN:Ada Lovelace\nEND:VCARD","user_id":123456789}}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let MessageContent::Contact(contact) = &message.content else {
                    panic!("{:?}", message.content);
                };
                assert_eq!(contact.phone_number, "+14155550123");
                assert_eq!(contact.display_name(), "Ada Lovelace");
                assert!(contact.vcard.contains("BEGIN:VCARD"));
                assert_eq!(contact.user_id, 123456789);
                assert_eq!(message.content.preview(), "👤 Ada Lovelace");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn message_contact_without_name_or_phone_is_unsupported() {
        let json = r#"{"@type":"updateNewMessage","message":{"id":112,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageContact","contact":{"@type":"contact","phone_number":"","first_name":"","last_name":"","vcard":"","user_id":0}}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => match &message.content {
                MessageContent::Unsupported { type_name } => {
                    assert_eq!(type_name, "messageContact")
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    // Phase 4.4: `messageDice` (schema 1.8.67 line 5231). `initial_state`
    // / `final_state` / `success_animation_frame_number` parse without
    // breaking and are dropped; `emoji` + `value` are kept.
    #[test]
    fn message_dice_parses_emoji_and_value() {
        let json = r#"{"@type":"updateNewMessage","message":{"id":115,"chat_id":17,"is_outgoing":false,"content":{"@type":"messageDice","initial_state":{"@type":"diceStickersRegular","sticker":{"@type":"sticker"}},"final_state":{"@type":"diceStickersRegular","sticker":{"@type":"sticker"}},"emoji":"🎲","value":4,"success_animation_frame_number":12}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let MessageContent::Dice(dice) = &message.content else {
                    panic!("{:?}", message.content);
                };
                assert_eq!(dice.emoji, "🎲");
                assert_eq!(dice.value, 4);
                assert_eq!(dice.face(), "🎲");
                assert_eq!(dice.label(), "🎲 4");
                assert_eq!(message.content.preview(), "🎲 4");
            }
            other => panic!("{other:?}"),
        }
    }

    // Phase 4.4 safe rule: `value` is required — a missing (or
    // non-integer) value can't be displayed honestly, so the message
    // becomes `Unsupported` instead of inventing a number.
    #[test]
    fn message_dice_without_value_is_unsupported() {
        let json = r#"{"@type":"updateNewMessage","message":{"id":116,"chat_id":17,"is_outgoing":false,"content":{"@type":"messageDice","emoji":"🎲"}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => match &message.content {
                MessageContent::Unsupported { type_name } => {
                    assert_eq!(type_name, "messageDice")
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    // Phase 4.4: an empty `emoji` falls back to the plain die rather than
    // rendering nothing (the value still parses).
    #[test]
    fn message_dice_empty_emoji_falls_back_to_die() {
        // Parser-level: emoji present but empty in the JSON (not constructed
        // directly) — the row falls back to 🎲.
        let json = r#"{"@type":"updateNewMessage","message":{"id":116,"chat_id":17,"is_outgoing":false,"content":{"@type":"messageDice","emoji":"","value":3}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let MessageContent::Dice(dice) = &message.content else {
                    panic!("{message:?}");
                };
                assert_eq!(dice.face(), "🎲");
                assert_eq!(dice.label(), "🎲 3");
            }
            other => panic!("{other:?}"),
        }
    }

    // Phase 4.3 safe rule: coordinates must be finite and in range; the
    // location is dropped otherwise (the message renders as
    // `Unsupported`). Note `serde_json` already rejects out-of-range
    // float literals (`1e999`) at the parse boundary, so infinities
    // can't reach `geo_location` from text; a huge-but-finite value
    // still fails the `|lat| <= 90` range check below.
    #[test]
    fn location_rejects_non_finite_coordinates() {
        assert!(
            serde_json::from_str::<serde_json::Value>(r#"{"latitude":1e999,"longitude":0.0}"#)
                .is_err()
        );
        let value: serde_json::Value =
            serde_json::from_str(r#"{"latitude":1e308,"longitude":0.0}"#).unwrap();
        assert!(geo_location(Some(&value)).is_none());
        assert!(geo_location(None).is_none());
    }

    #[test]
    fn location_rejects_out_of_range_coordinates() {
        assert!(
            geo_location(Some(
                &serde_json::json!({"latitude": 95.0, "longitude": 0.0})
            ))
            .is_none()
        );
        assert!(
            geo_location(Some(
                &serde_json::json!({"latitude": 0.0, "longitude": -190.0})
            ))
            .is_none()
        );
    }

    #[test]
    fn message_location_with_bad_coordinates_is_unsupported() {
        let json = r#"{"@type":"updateNewMessage","message":{"id":113,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageLocation","location":{"@type":"location","latitude":95.0,"longitude":200.0,"horizontal_accuracy":0}}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => match &message.content {
                MessageContent::Unsupported { type_name } => {
                    assert_eq!(type_name, "messageLocation")
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    // Phase 6: full `user` parse (schema 1.8.67 line 2403) — names,
    // username from `usernames.active_usernames` (no singular `username`
    // field in 1.8.67), phone, contact flag, status, and the
    // `profile_photo.small` file id.
    #[test]
    fn update_user_parses_full_user() {
        let json = r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","last_name":"Lovelace","usernames":{"@type":"usernames","active_usernames":["adalove"],"disabled_usernames":[],"editable_username":"adalove","collectible_usernames":[]},"phone_number":"+15550131","status":{"@type":"userStatusOnline","expires":9999999999},"profile_photo":{"@type":"profilePhoto","id":7,"small":{"@type":"file","id":41,"size":0,"expected_size":0,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_delete":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}},"big":{"@type":"file","id":42,"size":0,"expected_size":0,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_delete":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}},"minithumbnail":null,"has_animation":false,"is_personal":false},"accent_color_id":0,"background_custom_emoji_id":0,"upgraded_gift_colors":null,"profile_accent_color_id":-1,"profile_background_custom_emoji_id":0,"emoji_status":null,"is_contact":true,"is_mutual_contact":true,"is_close_friend":false,"verification_status":null,"is_premium":false,"is_support":false,"restriction_info":null,"active_story_state":null,"restricts_new_chats":false,"paid_message_star_count":0,"have_access":true,"type":{"@type":"userTypeRegular"},"language_code":"en","added_to_attachment_menu":false}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateUser { user_id, user } => {
                assert_eq!(user_id, UserId(31));
                assert_eq!(user.first_name, "Ada");
                assert_eq!(user.last_name, "Lovelace");
                assert_eq!(user.display_name(), "Ada Lovelace");
                assert_eq!(user.initials(), "AL");
                assert_eq!(user.username, "adalove");
                assert_eq!(user.phone_number, "+15550131");
                assert!(user.is_contact);
                assert!(!user.is_bot);
                assert_eq!(user.status, UserStatusKind::Online);
                assert!(user.status.is_online());
                assert_eq!(user.photo_small_file_id, 41);
            }
            other => panic!("{other:?}"),
        }
    }

    // Phase 6: status buckets map to their display text; offline with a
    // timestamp formats relative to "now"; unknown constructors → Empty.
    #[test]
    fn user_status_display_buckets() {
        assert_eq!(UserStatusKind::Online.display_at(1_700_000_000), "online");
        assert_eq!(
            UserStatusKind::Recently.display_at(1_700_000_000),
            "last seen recently"
        );
        assert_eq!(
            UserStatusKind::LastWeek.display_at(1_700_000_000),
            "last seen within a week"
        );
        assert_eq!(
            UserStatusKind::LastMonth.display_at(1_700_000_000),
            "last seen within a month"
        );
        assert_eq!(
            UserStatusKind::Offline {
                was_online: 1_699_999_970
            }
            .display_at(1_700_000_000),
            "last seen just now"
        );
        assert_eq!(
            UserStatusKind::Offline {
                was_online: 1_699_999_400
            }
            .display_at(1_700_000_000),
            "last seen 10m ago"
        );
        assert_eq!(
            UserStatusKind::Offline { was_online: 0 }.display_at(1_700_000_000),
            "last seen a long time ago"
        );
        assert!(UserStatusKind::Empty.display_at(1_700_000_000).is_empty());
    }

    #[test]
    fn update_user_status_parsed() {
        // Schema 1.8.67 line 10729.
        let env = parse_envelope(
            r#"{"@type":"updateUserStatus","user_id":31,"status":{"@type":"userStatusLastWeek","by_my_privacy_settings":false}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateUserStatus { user_id, status } => {
                assert_eq!(user_id, UserId(31));
                assert_eq!(status, UserStatusKind::LastWeek);
            }
            other => panic!("{other:?}"),
        }
        // Unknown status constructor → Empty, never a parse failure.
        let env = parse_envelope(
            r#"{"@type":"updateUserStatus","user_id":31,"status":{"@type":"userStatusFromTheFuture"}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateUserStatus { status, .. } => {
                assert_eq!(status, UserStatusKind::Empty)
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn users_response_parsed() {
        // `getContacts` → `users` (schema 1.8.67 lines 14520 / 2471).
        let env =
            parse_envelope(r#"{"@type":"users","@extra":"3","total_count":2,"user_ids":[31,32]}"#)
                .unwrap();
        match env.payload {
            EnvelopePayload::Users { user_ids } => assert_eq!(user_ids, vec![31, 32]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn user_full_info_bio_parsed() {
        // Phase 6: `bio:formattedText` is kept alongside `bot_info`.
        let env = parse_envelope(
            r#"{"@type":"userFullInfo","@extra":"4","block_list":null,"bio":{"@type":"formattedText","text":"CANARY bio text","entities":[]},"birthdate":null,"bot_info":null}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UserFullInfo {
                bio,
                bot_info,
                photo,
            } => {
                assert_eq!(bio, "CANARY bio text");
                assert!(bot_info.is_none());
                assert!(photo.is_none());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn user_full_info_photo_parsed_from_chat_photo() {
        // `userFullInfo.photo:chatPhoto` (schema 1.8.67, lines 1030/2468):
        // the preferred size is `type == "m"`; the file is kept.
        let file = |id: i32| {
            format!(
                r#"{{"@type":"file","id":{id},"size":100,"expected_size":100,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_delete":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}}}}"#
            )
        };
        let json = format!(
            r#"{{"@type":"userFullInfo","@extra":"9","bio":null,"bot_info":null,"photo":{{"@type":"chatPhoto","id":1,"added_date":1,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"s","photo":{s},"width":90,"height":90,"progressive_sizes":[]}},{{"@type":"photoSize","type":"m","photo":{m},"width":320,"height":320,"progressive_sizes":[]}}],"animation":null,"small_animation":null,"sticker":null}}}}"#,
            s = file(901),
            m = file(902),
        );
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::UserFullInfo { photo, .. } => {
                let photo = photo.expect("chatPhoto size");
                assert_eq!(photo.id.0, 902);
            }
            other => panic!("{other:?}"),
        }
        // No photo field at all → None.
        let env =
            parse_envelope(r#"{"@type":"userFullInfo","@extra":"10","bio":null,"bot_info":null}"#)
                .unwrap();
        match env.payload {
            EnvelopePayload::UserFullInfo { photo, .. } => assert!(photo.is_none()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn supergroup_full_info_parsed() {
        // `getSupergroupFullInfo` → `supergroupFullInfo` (schema 1.8.67
        // lines 11513 / 2792): description + member_count kept.
        let env = parse_envelope(
            r#"{"@type":"supergroupFullInfo","@extra":"5","description":"CANARY group description","member_count":1234,"administrator_count":2}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::SupergroupFullInfo {
                description,
                member_count,
                linked_chat_id,
                slow_mode_delay,
                slow_mode_delay_expires_in,
                my_boost_count,
                unrestrict_boost_count,
            } => {
                assert_eq!(description, "CANARY group description");
                assert_eq!(member_count, 1234);
                // Parity slice: no `linked_chat_id` → 0 (no discussion group).
                assert_eq!(linked_chat_id, 0);
                // Phase A1: slow-mode fields default to 0 when absent.
                assert_eq!(slow_mode_delay, 0);
                assert_eq!(slow_mode_delay_expires_in, 0.0);
                assert_eq!(my_boost_count, 0);
                assert_eq!(unrestrict_boost_count, 0);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn supergroup_full_info_parses_slow_mode_fields() {
        // Phase A1: `slow_mode_delay` / `slow_mode_delay_expires_in`
        // (schema 1.8.67, lines 2758–2759) and the boost bypass counts
        // (lines 2779–2780) are parsed from `supergroupFullInfo`.
        let env = parse_envelope(
            r#"{"@type":"supergroupFullInfo","@extra":"5","description":"d","member_count":10,"slow_mode_delay":30,"slow_mode_delay_expires_in":12.5,"my_boost_count":2,"unrestrict_boost_count":5}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::SupergroupFullInfo {
                slow_mode_delay,
                slow_mode_delay_expires_in,
                my_boost_count,
                unrestrict_boost_count,
                ..
            } => {
                assert_eq!(slow_mode_delay, 30);
                assert_eq!(slow_mode_delay_expires_in, 12.5);
                assert_eq!(my_boost_count, 2);
                assert_eq!(unrestrict_boost_count, 5);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn supergroup_full_info_parses_linked_chat_id() {
        // Parity slice: `linked_chat_id` (schema 1.8.67 line 2792) feeds the
        // channel header's "Discuss" affordance.
        let env = parse_envelope(
            r#"{"@type":"supergroupFullInfo","@extra":"5","description":"d","member_count":10,"linked_chat_id":77,"direct_messages_chat_id":0}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::SupergroupFullInfo { linked_chat_id, .. } => {
                assert_eq!(linked_chat_id, 77)
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn update_supergroup_full_info_parsed() {
        // Parity slice: `updateSupergroupFullInfo` (schema 1.8.67 line
        // 10750) carries its own `supergroup_id`.
        let env = parse_envelope(
            r#"{"@type":"updateSupergroupFullInfo","supergroup_id":13,"supergroup_full_info":{"@type":"supergroupFullInfo","description":"CANARY channel","member_count":12345,"linked_chat_id":14}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateSupergroupFullInfo {
                supergroup_id,
                description,
                member_count,
                linked_chat_id,
                slow_mode_delay,
                ..
            } => {
                assert_eq!(supergroup_id, 13);
                assert_eq!(description, "CANARY channel");
                assert_eq!(member_count, 12345);
                assert_eq!(linked_chat_id, 14);
                // Absent slow-mode fields default to 0.
                assert_eq!(slow_mode_delay, 0);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn update_supergroup_full_info_parses_slow_mode_fields() {
        // Phase A1: the nested `supergroup_full_info` also carries the
        // slow-mode fields (schema 1.8.67, lines 2758–2759).
        let env = parse_envelope(
            r#"{"@type":"updateSupergroupFullInfo","supergroup_id":13,"supergroup_full_info":{"@type":"supergroupFullInfo","description":"d","member_count":1,"slow_mode_delay":60,"slow_mode_delay_expires_in":44.0}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateSupergroupFullInfo {
                slow_mode_delay,
                slow_mode_delay_expires_in,
                ..
            } => {
                assert_eq!(slow_mode_delay, 60);
                assert_eq!(slow_mode_delay_expires_in, 44.0);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn update_supergroup_parses_own_status() {
        // Phase A1: `supergroup.status` (schema 1.8.67 line 2746) is the
        // viewer's own `chatMemberStatus*` — the slow-mode bypass signal.
        let env = parse_envelope(
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":false,"status":{"@type":"chatMemberStatusAdministrator"}}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateSupergroup {
                supergroup_id,
                status,
                ..
            } => {
                assert_eq!(supergroup_id, 16);
                assert_eq!(status, ChannelMemberStatus::Administrator);
            }
            other => panic!("{other:?}"),
        }
        // Missing status → Unknown (gated, no bypass).
        let env = parse_envelope(
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":17,"is_forum":false}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateSupergroup { status, .. } => {
                assert_eq!(status, ChannelMemberStatus::Unknown);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn update_new_chat_parses_photo_small() {
        // Parity slice: `chat.photo.small` (`chatPhotoInfo`, schema 1.8.67
        // lines 762/3627) is kept for the chat-list avatar.
        let json = r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Demo","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0,"photo":{"@type":"chatPhotoInfo","small":{"@type":"file","id":91,"size":24,"expected_size":24,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}},"big":{"@type":"file","id":92,"size":0,"expected_size":0,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}},"minithumbnail":null,"has_animation":false,"is_personal":false}}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewChat { chat_id, photo, .. } => {
                assert_eq!(chat_id.0, 11);
                let file = photo.expect("chat photo");
                assert_eq!(file.id.0, 91);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn update_new_chat_without_photo_has_none() {
        let json = r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Demo","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0,"photo":null}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewChat { photo, .. } => assert!(photo.is_none()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn update_chat_photo_parsed() {
        // Parity slice: `updateChatPhoto` (schema 1.8.67 line 10488).
        let json = r#"{"@type":"updateChatPhoto","chat_id":11,"photo":{"@type":"chatPhotoInfo","small":{"@type":"file","id":93,"size":24,"expected_size":24,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}},"big":{"@type":"file","id":94,"size":0,"expected_size":0,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}},"minithumbnail":null,"has_animation":false,"is_personal":false}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateChatPhoto { chat_id, photo } => {
                assert_eq!(chat_id.0, 11);
                assert_eq!(photo.map(|f| f.id.0), Some(93));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn add_contact_shape_uses_imported_contact() {
        // The `addContact` JSON shape lives in requests.rs, but the schema
        // contract it must match is pinned here: `addContact
        // user_id:int53 contact:importedContact share_phone_number:Bool =
        // Ok` with `importedContact phone_number:string first_name:string
        // last_name:string note:formattedText` (schema 1.8.67 lines 14513 /
        // 7382). This test guards the field list, not the builder.
        let line = include_str!("../../schema/td_api.tl")
            .lines()
            .find(|l| l.starts_with("addContact "))
            .expect("addContact in schema");
        assert!(
            line.contains("contact:importedContact"),
            "unexpected addContact signature: {line}"
        );
        let imported = include_str!("../../schema/td_api.tl")
            .lines()
            .find(|l| l.starts_with("importedContact "))
            .expect("importedContact in schema");
        for field in [
            "phone_number:string",
            "first_name:string",
            "last_name:string",
            "note:formattedText",
        ] {
            assert!(
                imported.contains(field),
                "importedContact missing {field}: {imported}"
            );
        }
    }

    #[test]
    fn update_chat_folders_parsed() {
        // Phase 7.1: `updateChatFolders` (schema 1.8.67 line 10606) carries
        // `vector<chatFolderInfo>` (line 3485); there is no `getChatFolders`
        // function in 1.8.67, so this update is the folder list.
        let env = parse_envelope(
            r#"{"@type":"updateChatFolders","chat_folders":[{"@type":"chatFolderInfo","id":3,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]},"animate_custom_emoji":false},"icon":{"@type":"chatFolderIcon","name":"Work"},"color_id":2,"is_shareable":false,"has_my_invite_links":false},{"@type":"chatFolderInfo","id":7,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"News","entities":[]},"animate_custom_emoji":false},"icon":null,"color_id":-1,"is_shareable":false,"has_my_invite_links":false}],"main_chat_list_position":0,"are_tags_enabled":false}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateChatFolders {
                folders,
                are_tags_enabled,
            } => {
                assert_eq!(folders.len(), 2);
                assert!(!are_tags_enabled);
                assert_eq!(
                    folders[0],
                    ChatFolderInfo {
                        id: 3,
                        name: "Work".into(),
                        icon_name: "Work".into(),
                        color_id: 2,
                    }
                );
                assert_eq!(
                    folders[1],
                    ChatFolderInfo {
                        id: 7,
                        name: "News".into(),
                        icon_name: String::new(),
                        color_id: -1,
                    }
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn chat_list_folder_parsed() {
        // `chatListFolder` (schema 1.8.67 line 3524) — folder membership
        // arrives in `chatPosition.list` / added-to / removed-from list.
        let env = parse_envelope(
            r#"{"@type":"updateChatPosition","chat_id":11,"position":{"@type":"chatPosition","list":{"@type":"chatListFolder","chat_folder_id":3},"order":"50","is_pinned":false}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateChatPosition(pos) => {
                assert_eq!(pos.list, ChatList::Folder(3));
                assert_eq!(pos.order, 50);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn update_chat_active_stories_parses_tray_fields() {
        // `updateChatActiveStories` (schema 1.8.67 line 10911): keeps the
        // tray sort key and read state; `can_be_archived` is dropped.
        let json = r#"{"@type":"updateChatActiveStories","active_stories":{"@type":"chatActiveStories","chat_id":11,"list":{"@type":"storyListMain"},"order":"9000","can_be_archived":true,"max_read_story_id":4,"stories":[{"@type":"storyInfo","story_id":5,"date":1700000000,"is_for_close_friends":false,"is_live":false},{"@type":"storyInfo","story_id":3,"date":1699990000,"is_for_close_friends":true,"is_live":false}]}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateChatActiveStories { active_stories } => {
                assert_eq!(active_stories.chat_id, 11);
                assert_eq!(active_stories.list, Some(StoryListView::Main));
                assert_eq!(active_stories.order, 9000);
                assert_eq!(active_stories.max_read_story_id, 4);
                assert_eq!(active_stories.stories.len(), 2);
                assert_eq!(active_stories.stories[0].story_id, 5);
                assert!(active_stories.stories[1].is_for_close_friends);
                assert!(active_stories.has_unread());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn chat_active_stories_null_list_and_no_unread() {
        // `list` may be null (schema line 6778); all stories read.
        let json = r#"{"@type":"chatActiveStories","chat_id":12,"list":null,"order":"0","can_be_archived":false,"max_read_story_id":9,"stories":[{"@type":"storyInfo","story_id":9,"date":1,"is_for_close_friends":false,"is_live":false}]}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::ChatActiveStories { active_stories } => {
                assert_eq!(active_stories.list, None);
                assert!(!active_stories.has_unread());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn story_chosen_reaction_interactions_and_flags_parsed() {
        // Phase 9.2: `chosen_reaction_type` (reactionTypeEmoji), the
        // `storyInteractionInfo` counters, and the `can_be_deleted` /
        // `can_be_replied` / `can_get_interactions` gates (schema 1.8.67
        // lines 6712 / 6742).
        let size = story_photo_file_json(61, "\"\"", false);
        let json = format!(
            r#"{{"@type":"story","id":7,"poster_chat_id":11,"date":1700000000,"content":{{"@type":"storyContentPhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{size}]}}}},"chosen_reaction_type":{{"@type":"reactionTypeEmoji","emoji":"❤"}},"interaction_info":{{"@type":"storyInteractionInfo","view_count":42,"forward_count":3,"reaction_count":7,"recent_viewer_user_ids":[]}},"can_be_deleted":true,"can_be_replied":true,"can_get_interactions":true,"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
        );
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::Story { story, .. } => {
                assert_eq!(story.chosen_reaction_emoji.as_deref(), Some("❤"));
                let info = story.interaction_info.expect("interaction_info");
                assert!(info.any_nonzero());
                assert_eq!(info.view_count, 42);
                assert_eq!(info.forward_count, 3);
                assert_eq!(info.reaction_count, 7);
                assert!(story.can_be_deleted);
                assert!(story.can_be_replied);
                assert!(story.can_get_interactions);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn story_reaction_absent_or_non_emoji_parses_to_none() {
        // `chosen_reaction_type: null` and custom-emoji / paid reactions
        // all parse to `None` — the viewer only renders emoji reactions.
        let reaction_json = |reaction: &str| {
            format!(
                r#"{{"@type":"story","id":7,"poster_chat_id":11,"date":1,"content":{{"@type":"storyContentUnsupported"}},"chosen_reaction_type":{reaction},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
            )
        };
        for reaction in [
            "null",
            r#"{"@type":"reactionTypeCustomEmoji","custom_emoji_id":"123"}"#,
            r#"{"@type":"reactionTypePaid"}"#,
            r#"{"@type":"reactionTypeEmoji","emoji":""}"#,
        ] {
            let env = parse_envelope(&reaction_json(reaction)).unwrap();
            match env.payload {
                EnvelopePayload::Story { story, .. } => {
                    assert_eq!(story.chosen_reaction_emoji, None, "reaction {reaction}");
                }
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn story_interaction_info_absent_stays_none() {
        let json = r#"{"@type":"story","id":7,"poster_chat_id":11,"date":1,"content":{"@type":"storyContentUnsupported"},"caption":{"@type":"formattedText","text":"","entities":[]}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::Story { story, .. } => {
                assert_eq!(story.interaction_info, None);
                assert!(!story.can_be_deleted);
                assert!(!story.can_be_replied);
                assert!(!story.can_get_interactions);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn update_story_deleted_parsed() {
        // `updateStoryDeleted` (schema 1.8.67 line 10898).
        let env = parse_envelope(
            r#"{"@type":"updateStoryDeleted","story_poster_chat_id":11,"story_id":7}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateStoryDeleted {
                poster_chat_id,
                story_id,
            } => {
                assert_eq!(poster_chat_id, 11);
                assert_eq!(story_id, 7);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn update_story_post_succeeded_parsed() {
        // `updateStoryPostSucceeded` (schema 1.8.67 line 10901).
        let json = r#"{"@type":"updateStoryPostSucceeded","story":{"@type":"story","id":7,"poster_chat_id":11,"date":1,"content":{"@type":"storyContentUnsupported"},"caption":{"@type":"formattedText","text":"","entities":[]}},"old_story_id":6}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateStoryPostSucceeded {
                story,
                old_story_id,
                ..
            } => {
                assert_eq!(story.id, 7);
                assert_eq!(story.poster_chat_id, 11);
                assert_eq!(old_story_id, 6);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn update_story_post_failed_parsed() {
        // `updateStoryPostFailed` (schema 1.8.67 line 10907).
        let json = r#"{"@type":"updateStoryPostFailed","story":{"@type":"story","id":7,"poster_chat_id":11,"date":1,"content":{"@type":"storyContentUnsupported"},"caption":{"@type":"formattedText","text":"","entities":[]}},"error":{"@type":"error","code":400,"message":"STORY_SEND_FAILED"},"error_type":{"@type":"canPostStoryResultOk"}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateStoryPostFailed { story, error } => {
                assert_eq!(story.id, 7);
                assert_eq!(story.poster_chat_id, 11);
                assert_eq!(error.code, 400);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn available_reactions_parsed_and_custom_emoji_dropped() {
        // `availableReactions` (schema 1.8.67 line 7330): the story
        // picker keeps emoji reactions; custom-emoji rows are dropped.
        let json = r#"{"@type":"availableReactions","top_reactions":[{"@type":"availableReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"needs_premium":false},{"@type":"availableReaction","type":{"@type":"reactionTypeCustomEmoji","custom_emoji_id":"123"},"needs_premium":true},{"@type":"availableReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"needs_premium":false}],"recent_reactions":[],"popular_reactions":[],"allow_custom_emoji":false,"are_tags":false,"unavailability_reason":null}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::StoryAvailableReactions { reactions } => {
                assert_eq!(reactions.len(), 2);
                assert_eq!(reactions[0].emoji, "❤");
                assert!(!reactions[0].needs_premium);
                assert_eq!(reactions[1].emoji, "👍");
            }
            other => panic!("{other:?}"),
        }
    }

    fn story_photo_file_json(id: i32, path: &str, completed: bool) -> String {
        format!(
            r#"{{"@type":"photoSize","type":"x","photo":{{"@type":"file","id":{id},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":{path},"can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":{completed},"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}},"width":800,"height":600,"progressive_sizes":[]}}"#,
            path = serde_json::to_string(path).unwrap(),
            completed = completed,
        )
    }

    #[test]
    fn get_story_photo_parses_content_and_caption() {
        let size = story_photo_file_json(61, "\"\"", false);
        let json = format!(
            r#"{{"@type":"story","id":5,"poster_chat_id":11,"poster_id":null,"date":1700000000,"is_being_posted":false,"is_being_edited":false,"is_edited":false,"is_posted_to_chat_page":false,"is_visible_only_for_self":false,"can_be_added_to_album":false,"can_be_deleted":false,"can_be_edited":false,"can_be_forwarded":true,"can_be_replied":false,"can_set_privacy_settings":false,"can_toggle_is_posted_to_chat_page":false,"can_get_statistics":false,"can_get_interactions":false,"has_expired_viewers":false,"repost_info":null,"interaction_info":null,"chosen_reaction_type":null,"privacy_settings":null,"content":{{"@type":"storyContentPhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{size}]}}}},"areas":[],"caption":{{"@type":"formattedText","text":"CANARY_STORY_caption","entities":[]}},"album_ids":[]}}"#,
        );
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::Story { story, files } => {
                assert_eq!(story.id, 5);
                assert_eq!(story.poster_chat_id, 11);
                assert_eq!(story.date, 1700000000);
                assert_eq!(story.caption, "CANARY_STORY_caption");
                match story.content {
                    StoryContentView::Photo { sizes } => {
                        assert_eq!(sizes.len(), 1);
                        assert_eq!(sizes[0].file_id, FileId(61));
                        assert_eq!(sizes[0].width, 800);
                    }
                    other => panic!("{other:?}"),
                }
                assert!(files.iter().any(|f| f.id == FileId(61)));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn get_story_video_parses_thumb_and_duration() {
        // `storyVideo` (schema 1.8.67 line 6633): `duration` is a double,
        // the thumbnail is a bare `thumbnail` constructor.
        let thumb = r#"{"@type":"thumbnail","format":{"@type":"thumbnailFormatJpeg"},"width":320,"height":240,"file":{"@type":"file","id":71,"size":10,"expected_size":10,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}}}"#;
        let clip = r#"{"@type":"file","id":72,"size":100,"expected_size":100,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"y","unique_id":"v","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":100}}"#;
        let json = format!(
            r#"{{"@type":"story","id":6,"poster_chat_id":11,"date":1700000000,"content":{{"@type":"storyContentVideo","video":{{"@type":"storyVideo","duration":12.4,"width":720,"height":1280,"has_stickers":false,"is_animation":false,"minithumbnail":null,"thumbnail":{thumb},"preload_prefix_size":0,"cover_frame_timestamp":0.0,"video":{clip}}},"alternative_video":null}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
        );
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::Story { story, files } => {
                match story.content {
                    StoryContentView::Video {
                        thumb_file_id,
                        thumb_width,
                        thumb_height,
                        duration_secs,
                        file_id,
                    } => {
                        assert_eq!(thumb_file_id, Some(FileId(71)));
                        assert_eq!(thumb_width, 320);
                        assert_eq!(thumb_height, 240);
                        assert_eq!(duration_secs, 12);
                        assert_eq!(file_id, FileId(72));
                    }
                    other => panic!("{other:?}"),
                }
                assert!(files.iter().any(|f| f.id == FileId(71)));
                assert!(files.iter().any(|f| f.id == FileId(72)));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn story_live_and_unsupported_degrade_to_placeholder() {
        for (content, is_live) in [
            (
                r#"{"@type":"storyContentLive","group_call_id":7,"is_rtmp_stream":false}"#,
                true,
            ),
            (r#"{"@type":"storyContentUnsupported"}"#, false),
            (r#"{"@type":"storyContentQuantum"}"#, false),
        ] {
            let json = format!(
                r#"{{"@type":"updateStory","story":{{"@type":"story","id":8,"poster_chat_id":11,"date":1,"content":{content},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}"#,
            );
            let env = parse_envelope(&json).unwrap();
            match env.payload {
                EnvelopePayload::Story { story, .. } => match (&story.content, is_live) {
                    (StoryContentView::Live, true) => {}
                    (StoryContentView::Unsupported, false) => {}
                    (other, _) => panic!("{other:?}"),
                },
                other => panic!("{other:?}"),
            }
        }
    }
}

#[cfg(test)]
mod notification_sound_tests {
    use super::*;

    const SOUND_FILE: &str = r#"{"@type":"file","id":77,"size":12,"expected_size":12,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":12}}"#;

    #[test]
    fn notification_sounds_parsed() {
        // `getSavedNotificationSounds` response (schema 1.8.67 lines 8857–8860).
        let json = format!(
            r#"{{"@type":"notificationSounds","notification_sounds":[{{"@type":"notificationSound","id":99,"duration":2,"date":1700000000,"title":"Chime","data":"","sound":{}}}]}}"#,
            SOUND_FILE
        );
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::NotificationSounds { sounds } => {
                assert_eq!(sounds.len(), 1);
                assert_eq!(sounds[0].id, 99);
                assert_eq!(sounds[0].title, "Chime");
                assert_eq!(sounds[0].duration, 2);
                assert_eq!(sounds[0].sound.id.0, 77);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn update_saved_notification_sounds_parsed() {
        // Schema 1.8.67 line 10947.
        let env = parse_envelope(
            r#"{"@type":"updateSavedNotificationSounds","notification_sound_ids":[7,8]}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateSavedNotificationSounds { sound_ids } => {
                assert_eq!(sound_ids, vec![7, 8]);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn scope_notification_settings_parsed() {
        // Schema 1.8.67 line 3375.
        let env = parse_envelope(
            r#"{"@type":"scopeNotificationSettings","mute_for":3600,"sound_id":-1,"show_preview":true,"use_default_mute_stories":true,"mute_stories":false,"story_sound_id":-1,"show_story_poster":true,"disable_pinned_message_notifications":false,"disable_mention_notifications":true}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::ScopeNotificationSettings { settings, .. } => {
                assert_eq!(settings.mute_for, 3600);
                assert_eq!(settings.sound_id, -1);
                assert!(settings.show_preview);
                assert!(settings.disable_mention_notifications);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn update_scope_notification_settings_parsed() {
        // Schema 1.8.67 line 10668; scope constructor lines 3337–3343.
        let env = parse_envelope(
            r#"{"@type":"updateScopeNotificationSettings","scope":{"@type":"notificationSettingsScopeGroupChats"},"notification_settings":{"@type":"scopeNotificationSettings","mute_for":0,"sound_id":0,"show_preview":false,"use_default_mute_stories":true,"mute_stories":false,"story_sound_id":-1,"show_story_poster":true,"disable_pinned_message_notifications":false,"disable_mention_notifications":false}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateScopeNotificationSettings { scope, settings } => {
                assert_eq!(scope, NotificationSettingsScope::GroupChats);
                assert_eq!(settings.sound_id, 0);
                assert!(!settings.show_preview);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn unknown_scope_is_rejected() {
        assert_eq!(
            parse_notification_settings_scope(Some("notificationSettingsScopeBots")),
            None
        );
        assert_eq!(
            parse_notification_settings_scope(Some("notificationSettingsScopePrivateChats")),
            Some(NotificationSettingsScope::PrivateChats)
        );
    }

    #[test]
    fn schema_pins_call_constructors() {
        // Every constructor this slice relies on must exist verbatim in
        // the pinned schema (1.8.67) — never invent constructors or
        // fields. Lines: updateCall :10816, updateNewCallSignalingData
        // :10862, callId :7034, createCall :14212, acceptCall :14215,
        // sendCallSignalingData :14218, discardCall :14227,
        // sendCallRating :14233, sendCallDebugInformation :14237,
        // call :7287, callProtocol :7008, states :7058–7086,
        // discard reasons :6984–6999, problems :7253–7277.
        let schema = include_str!("../../schema/td_api.tl");
        for line in [
            "updateCall call:call = Update;",
            "updateNewCallSignalingData call_id:int32 data:bytes = Update;",
            "callId id:int32 = CallId;",
            "call id:int32 unique_id:int64 user_id:int53 is_outgoing:Bool is_video:Bool state:CallState = Call;",
            "callProtocol udp_p2p:Bool udp_reflector:Bool min_layer:int32 max_layer:int32 library_versions:vector<string> = CallProtocol;",
            "createCall user_id:int53 protocol:callProtocol is_video:Bool = CallId;",
            "acceptCall call_id:int32 protocol:callProtocol = Ok;",
            "sendCallSignalingData call_id:int32 data:bytes = Ok;",
            "discardCall call_id:int32 is_disconnected:Bool invite_link:string duration:int32 is_video:Bool connection_id:int64 = Ok;",
            "sendCallRating call_id:InputCall rating:int32 comment:string problems:vector<CallProblem> = Ok;",
            "sendCallDebugInformation call_id:InputCall debug_information:string = Ok;",
            "callStatePending is_created:Bool is_received:Bool = CallState;",
            "callStateExchangingKeys = CallState;",
            "callStateHangingUp = CallState;",
            "callStateDiscarded reason:CallDiscardReason need_rating:Bool need_debug_information:Bool need_log:Bool = CallState;",
            "callDiscardReasonMissed = CallDiscardReason;",
            "callDiscardReasonDeclined = CallDiscardReason;",
            "callDiscardReasonHungUp = CallDiscardReason;",
            "inputCallDiscarded call_id:int32 = InputCall;",
        ] {
            assert!(
                schema.lines().any(|l| l == line),
                "schema pin missing: {line}"
            );
        }
        for prefix in [
            "callStateReady protocol:callProtocol",
            "callStateError error:error = CallState;",
            "callDiscardReasonEmpty = CallDiscardReason;",
        ] {
            assert!(
                schema.lines().any(|l| l.starts_with(prefix)),
                "schema pin missing: {prefix}"
            );
        }
    }

    /// Phase C1: `updateCall` parses the full `call` record in every
    /// state; `updateNewCallSignalingData` keeps base64 bytes; `callId`
    /// is the `createCall` answer.
    #[test]
    fn call_updates_parsed_in_every_state() {
        let pending = r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#;
        match parse_envelope(pending).unwrap().payload {
            EnvelopePayload::UpdateCall { call } => {
                assert_eq!(call.id, 77);
                assert_eq!(call.unique_id, 99);
                assert_eq!(call.user_id, 41);
                assert!(!call.is_outgoing);
                assert!(!call.is_video);
                assert_eq!(
                    call.state,
                    CallState::Pending {
                        is_created: true,
                        is_received: false
                    }
                );
                assert!(!call.state.is_terminal());
            }
            other => panic!("unexpected {other:?}"),
        }
        for (state_json, terminal) in [
            (r#"{"@type":"callStateExchangingKeys"}"#, false),
            (
                r#"{"@type":"callStateReady","protocol":{"@type":"callProtocol","udp_p2p":true,"udp_reflector":true,"min_layer":65,"max_layer":92,"library_versions":[]},"servers":[],"config":"{}","encryption_key":"","emojis":[],"allow_p2p":false,"is_group_call_supported":false,"custom_parameters":"{}"}"#,
                false,
            ),
            (r#"{"@type":"callStateHangingUp"}"#, false),
            (
                r#"{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":true,"need_debug_information":false,"need_log":false}"#,
                true,
            ),
            (
                r#"{"@type":"callStateError","error":{"@type":"error","code":4005000,"message":"CALL_TIMEOUT"}}"#,
                true,
            ),
            (r#"{"@type":"callStateFuture"}"#, false),
        ] {
            let json = format!(
                r#"{{"@type":"updateCall","call":{{"@type":"call","id":78,"unique_id":"100","user_id":41,"is_outgoing":true,"is_video":false,"state":{state_json}}}}}"#
            );
            match parse_envelope(&json).unwrap().payload {
                EnvelopePayload::UpdateCall { call } => {
                    assert!(call.is_outgoing);
                    assert_eq!(call.state.is_terminal(), terminal, "for {state_json}");
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        // Discard reason summaries.
        assert_eq!(CallDiscardReason::Missed.summary(false), "Missed call");
        assert_eq!(CallDiscardReason::Missed.summary(true), "Call not answered");
        assert_eq!(
            CallDiscardReason::Declined.summary(false),
            "You declined the call"
        );
        assert_eq!(CallDiscardReason::Declined.summary(true), "Declined");
        // Signaling data arrives as base64 bytes.
        let sig = r#"{"@type":"updateNewCallSignalingData","call_id":77,"data":"AAEC"}"#;
        match parse_envelope(sig).unwrap().payload {
            EnvelopePayload::UpdateNewCallSignalingData { call_id, data } => {
                assert_eq!(call_id, 77);
                assert_eq!(data, vec![0x00, 0x01, 0x02]);
            }
            other => panic!("unexpected {other:?}"),
        }
        // `callId` is the `createCall` answer.
        let id = r#"{"@type":"callId","id":77,"@extra":"9"}"#;
        match parse_envelope(id).unwrap().payload {
            EnvelopePayload::CallId { id } => assert_eq!(id, 77),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn schema_pins_notification_sound_constructors() {
        // Every constructor this slice relies on must exist verbatim in the
        // pinned schema (1.8.67) — never invent constructors or fields.
        let schema = include_str!("../../schema/td_api.tl");
        for line in [
            "notificationSound id:int64 duration:int32 date:int32 title:string data:string sound:file = NotificationSound;",
            "notificationSounds notification_sounds:vector<notificationSound> = NotificationSounds;",
            "updateSavedNotificationSounds notification_sound_ids:vector<int64> = Update;",
            "getSavedNotificationSound notification_sound_id:int64 = NotificationSound;",
            "getSavedNotificationSounds = NotificationSounds;",
            "addSavedNotificationSound sound:InputFile = NotificationSound;",
            "removeSavedNotificationSound notification_sound_id:int64 = Ok;",
            "fileTypeNotificationSound = FileType;",
        ] {
            assert!(
                schema.lines().any(|l| l == line),
                "schema pin missing: {line}"
            );
        }
    }
}
