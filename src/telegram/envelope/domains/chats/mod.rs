//! TDLib updates and answers for chat-level look and actions: backgrounds, themes, deep links, action bar.
mod parse;

use crate::ids::{ChatId, MessageId};
use crate::telegram::envelope::*;
use crate::telegram::name_accent::NameAccentColor;
use crate::telegram::profile_accent::ProfileAccentColor;
use crate::text::TextEntity;
pub(crate) use parse::parse_chats_payload;

/// Payloads for chat-level look and actions: backgrounds, themes, deep links, action bar; wrapped as
/// [`EnvelopePayload::Chats`].
// Sized like the flat enum before the domain split; boxing the big
// variant would change every constructor and pattern.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum ChatsPayload {
    UpdateChatTitle {
        chat_id: ChatId,
        title: String,
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
    /// B15: `updateChatUnreadPollVoteCount` (schema 1.8.67, line 10573) —
    /// the row's poll-vote badge. `updateMessageContainsUnreadPollVotes`
    /// (line 10457) carries the same new chat counter and folds into it.
    UpdateChatUnreadPollVoteCount {
        chat_id: ChatId,
        unread_poll_vote_count: i32,
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
    UpdateNewChat {
        chat_id: ChatId,
        title: String,
        kind: ChatKind,
        /// `chat.accent_color_id` and `chat.background_custom_emoji_id`
        /// (schema 1.8.67, line 3937): the channel's or group's name color
        /// and the emoji repeated behind replies to its posts.
        accent: ChatAccent,
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
        /// `chat.available_reactions` (schema 1.8.67, line 3627); refreshed
        /// by `updateChatAvailableReactions`. `None` when absent.
        available_reactions: Option<ChatAvailableReactions>,
        /// `chat.has_scheduled_messages` (schema 1.8.67, line 3627) — the
        /// chat has scheduled messages; refreshed by
        /// `updateChatHasScheduledMessages`.
        has_scheduled_messages: bool,
        /// `chat.message_sender_id` (schema 1.8.67, line 3627) — the "send
        /// as" identity selected for the chat; `None` when the user can't
        /// change it. Refreshed by `updateChatMessageSender`.
        message_sender: Option<MessageSender>,
        /// `chat.is_translatable` (schema 1.8.67, lines 3599 / 3627) —
        /// translation of the chat's messages must be suggested.
        /// Refreshed by `updateChatIsTranslatable` (line 10585).
        is_translatable: bool,
        /// `chat.view_as_topics` (schema 1.8.67, line 3627); `None` when the
        /// field is absent. Refreshed by `updateChatViewAsTopics`.
        view_as_topics: Option<bool>,
        /// `chat.default_disable_notification` (schema line 3937); `None`
        /// when absent. Refreshed by `updateChatDefaultDisableNotification`.
        default_disable_notification: Option<bool>,
        /// `chat.background`: the chat's own wallpaper, if any.
        background: Option<ChatBackground>,
        /// `chat.theme` when it is an emoji theme.
        theme_name: Option<String>,
        /// `chat.reply_markup_message_id` (schema 1.8.67, line 3624): the
        /// message whose keyboard the chat shows; 0 for none.
        reply_markup_message_id: MessageId,
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
        /// B15: `chat.unread_poll_vote_count` (schema 1.8.67, line 3613).
        unread_poll_vote_count: i32,
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
    /// Slice A12: `updateProfileAccentColors` (TDLib 1.8.67,
    /// `schema/td_api.tl:10964`) — the accent palette plus the ids
    /// `setProfileAccentColor` accepts. Stored in `Session`; drives the
    /// edit-profile accent picker.
    UpdateProfileAccentColors {
        colors: Vec<ProfileAccentColor>,
        available_ids: Vec<i32>,
    },
    /// `updateAccentColors`: the name-color palette (ids 7+ and their
    /// built-in fallbacks). Stored in `Session`.
    UpdateAccentColors {
        colors: Vec<NameAccentColor>,
        available_ids: Vec<i32>,
    },
    /// `updateChatNotificationSettings` — chat mute / sound exception changed.
    UpdateChatNotificationSettings {
        chat_id: ChatId,
        notification_settings: ChatNotificationSettings,
    },
    /// `updateChatDefaultDisableNotification` (schema line 10942): the
    /// chat's sends are silent unless the user turns that off.
    UpdateChatDefaultDisableNotification {
        chat_id: ChatId,
        default_disable_notification: bool,
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
    /// B10: `chatPhotos` — `getUserProfilePhotos` (schema 1.8.67, line
    /// 14591), newest first.
    ChatPhotos {
        total_count: i32,
        photos: Vec<ParsedProfilePhoto>,
    },
    /// `updateChatOnlineMemberCount` — sent for opened groups.
    UpdateChatOnlineMemberCount {
        chat_id: i64,
        online_member_count: i32,
    },
    /// `updateChatHasProtectedContent` (schema 1.8.67, line 10582) — the
    /// chat's `has_protected_content` changed.
    UpdateChatHasProtectedContent {
        chat_id: i64,
        has_protected_content: bool,
    },
    /// B7: `updateChatAvailableReactions` (schema 1.8.67, line 10532).
    UpdateChatAvailableReactions {
        chat_id: i64,
        available_reactions: ChatAvailableReactions,
    },
    /// `updateChatAccentColors` (schema 1.8.67, line 10860): the chat's
    /// name color or reply emoji changed.
    UpdateChatAccentColors {
        chat_id: i64,
        accent: ChatAccent,
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
    /// `internalLinkType*` — the `getInternalLinkType` answer
    /// (`parity:deeplink-internal-link-type`).
    InternalLinkType(crate::deep_link_types::InternalLink),
    /// `messageLinkInfo` (schema line 9677) — the `getMessageLinkInfo`
    /// answer. `chat_id` is 0 when the link points nowhere the account can
    /// see.
    MessageLinkInfo {
        chat_id: i64,
        message_id: i64,
        media_timestamp: Option<i32>,
        thread_id: Option<i64>,
    },
    /// `chatBoostLinkInfo` — the `getChatBoostLinkInfo` answer. `chat_id` is
    /// 0 when the link does not name a channel the account can see.
    ChatBoostLinkInfo {
        chat_id: i64,
    },
    /// `backgrounds` — the answer of `getInstalledBackgrounds`.
    Backgrounds(Vec<Background>),
    /// `background` — the answer of `setDefaultBackground`.
    Background(Background),
    /// `updateDefaultBackground` (`schema/td_api.tl:11317`): the account's
    /// default wallpaper for the light or dark theme changed.
    UpdateDefaultBackground {
        for_dark_theme: bool,
        background: Background,
    },
    /// `updateChatBackground` (`schema/td_api.tl:10924`); `None` when the
    /// chat's wallpaper was removed.
    UpdateChatBackground {
        chat_id: ChatId,
        background: Option<ChatBackground>,
    },
    /// `updateChatTheme` (`schema/td_api.tl:10927`); `None` for no emoji
    /// theme.
    UpdateChatTheme {
        chat_id: ChatId,
        theme_name: Option<String>,
    },
    /// `updateEmojiChatThemes` (`schema/td_api.tl:11320`).
    UpdateEmojiChatThemes(Vec<EmojiChatTheme>),
}
