use crate::ids::{ChatId, FileId, MessageId, RequestId, UserId};
use crate::text::{TextEntity, TextEntityKind, utf16_to_utf8_offset};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Deserialize;
use serde_json::Value;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub type_name: String,
    pub extra: Option<RequestId>,
    pub client_id: Option<i32>,
    pub payload: EnvelopePayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    },
    /// `updateChatDraftMessage`. Positions are the new chat-list orders.
    UpdateChatDraftMessage {
        chat_id: ChatId,
        draft: Option<ChatDraft>,
        positions: Vec<ChatPositionUpdate>,
    },
    /// `updateUser` — only the bot bit is kept (private-chat draft gate).
    UpdateUser {
        user_id: UserId,
        is_bot: bool,
    },
    /// `updateChatNotificationSettings` — chat mute / sound exception changed.
    UpdateChatNotificationSettings {
        chat_id: ChatId,
        notification_settings: ChatNotificationSettings,
    },
    /// `updateChatAction` — peer activity (`chatActionTyping` / `chatActionCancel`).
    UpdateChatAction {
        chat_id: ChatId,
        sender: MessageSender,
        action: ChatAction,
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
    /// request's chat in `Session::apply`, so only `bot_info` (from
    /// `userFullInfo.bot_info:botInfo`, line 2468) is kept.
    UserFullInfo {
        bot_info: Option<BotInfo>,
    },
    /// `updateUserFullInfo` — full info changed (schema 1.8.67, line 10744);
    /// the user id is explicit here.
    UpdateUserFullInfo {
        user_id: UserId,
        bot_info: Option<BotInfo>,
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
    pub fn is_supported_cloud_chat(&self) -> bool {
        match self {
            ChatKind::Private { .. } | ChatKind::BasicGroup { .. } => true,
            // Phase 2.2: broadcast channels are ungated (sponsored-content
            // handling landed in 2.1).
            ChatKind::Supergroup { .. } => true,
            ChatKind::Secret { .. } | ChatKind::Unknown => false,
        }
    }

    pub fn gate_reason(&self) -> Option<&'static str> {
        match self {
            ChatKind::Secret { .. } => Some("Secret chats are out of scope for this client."),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatPositionUpdate {
    pub chat_id: ChatId,
    pub list: ChatList,
    pub order: i64,
    pub is_pinned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatList {
    Main,
    Archive,
    Folder(i32),
    Unknown,
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
            Ok(EnvelopePayload::UpdateUser {
                user_id: UserId(int53(user.get("id"))?),
                is_bot: user
                    .get("type")
                    .and_then(|t| t.get("@type"))
                    .and_then(Value::as_str)
                    == Some("userTypeBot"),
            })
        }
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
            })
        }
        "ok" => Ok(EnvelopePayload::Ok),
        "callbackQueryAnswer" => Ok(EnvelopePayload::CallbackQueryAnswer(
            parse_callback_query_answer(&value),
        )),
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
        }),
        "updateUserFullInfo" => Ok(EnvelopePayload::UpdateUserFullInfo {
            user_id: UserId(int53(value.get("user_id"))?),
            bot_info: parse_bot_info(
                value
                    .get("user_full_info")
                    .and_then(|info| info.get("bot_info")),
            ),
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
        content,
        files,
        reply_to: parse_reply_to(value.get("reply_to")),
        forward_info: parse_forward_info(value.get("forward_info")),
        interaction_info: parse_interaction_info(value.get("interaction_info")),
        reply_markup: parse_reply_markup(value.get("reply_markup")),
    })
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
            EnvelopePayload::UserFullInfo { bot_info } => {
                let info = bot_info.expect("bot_info");
                assert_eq!(info.short_description, "A demo bot");
                assert_eq!(info.description, "This bot demonstrates the info panel.");
                assert_eq!(info.commands.len(), 2);
                assert_eq!(info.commands[0].command, "start");
                assert_eq!(info.commands[0].description, "Start the bot");
                assert_eq!(info.commands[1].command, "help");
            }
            other => panic!("{other:?}"),
        }
        // Non-bot full info: `bot_info` null → None.
        let env = parse_envelope(
            r#"{"@type":"userFullInfo","@extra":"8","block_list":null,"bot_info":null}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UserFullInfo { bot_info } => assert!(bot_info.is_none()),
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
            EnvelopePayload::UpdateUserFullInfo { user_id, bot_info } => {
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
            EnvelopePayload::UpdateUser { user_id, is_bot } => {
                assert_eq!(user_id, UserId(11));
                assert!(is_bot);
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
}
