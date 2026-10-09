use super::*;
use serde_json::Value;

/// One `forumTopic` (TDLib 1.8.67, `schema/td_api.tl:3968` + `forumTopicInfo`
/// at 3953). Only the fields the topic list / topic view need are kept;
/// dropped fields are documented in the Phase 5.1 DECISIONS entry:
/// `icon` (custom-emoji topic icons are not rendered), `creation_date`,
/// `creator_id`, `is_outgoing`, `is_name_implicit`,
/// `last_read_inbox/outbox_message_id`, `unread_mention_count`,
/// `unread_reaction_count`, `unread_poll_vote_count`,
/// `notification_settings`, `draft_message`. Slice G2 added
/// `is_hidden` back (`forumTopicInfo.is_hidden`, schema line 3953)
/// for the General topic Hide/Show action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForumTopic {
    pub forum_topic_id: i32,
    pub name: String,
    pub is_general: bool,
    pub is_closed: bool,
    pub is_pinned: bool,
    /// Slice G2: `forumTopicInfo.is_hidden` (schema 1.8.67, line 3953)
    /// — "True, if the topic is hidden above the topic list and
    /// closed; for General topic only". Drives the Hide/Show action.
    pub is_hidden: bool,
    pub unread_count: i32,
    /// Schema `forumTopic.order` — topics sort by order descending.
    pub order: i64,
    /// Cheap preview of `forumTopic.last_message` via
    /// `MessageContent::preview`; empty when there is no last message.
    pub last_message_preview: String,
    /// Subsection tabs: `forumTopicIcon.color` (RGB, schema 1.8.67 line
    /// 3939) — the round topic icon's fill.
    pub icon_color: i32,
    /// Subsection tabs: `forumTopicIcon.custom_emoji_id` (0 = none). Kept
    /// so the icon can be upgraded later; the tabs draw the letter icon.
    pub icon_custom_emoji_id: i64,
    /// Subsection tabs: `forumTopic.last_message.id` (0 = none) — the
    /// "Mark as read" target and the unread bookkeeping anchor.
    pub last_message_id: i64,
    /// Subsection tabs: `forumTopic.last_read_inbox_message_id`.
    pub last_read_inbox_message_id: i64,
    /// Subsection tabs: `forumTopic.notification_settings` — drives the
    /// tab menu's Mute / Unmute (`setForumTopicNotificationSettings`).
    pub notification_settings: ChatNotificationSettings,
    /// `forumTopic.unread_mention_count` — gates "Mark all mentions as read".
    pub unread_mention_count: i32,
    /// `forumTopic.unread_reaction_count` — gates "Read all reactions".
    pub unread_reaction_count: i32,
}

/// Subsection tabs: the `forumTopicInfo` fields that
/// `updateForumTopicInfo` refreshes (schema 1.8.67, lines 3953 / 10652).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForumTopicInfoUpdate {
    pub chat_id: i64,
    pub forum_topic_id: i32,
    pub name: String,
    pub icon_color: i32,
    pub icon_custom_emoji_id: i64,
    pub is_general: bool,
    pub is_closed: bool,
    pub is_hidden: bool,
}

/// Subsection tabs: `updateForumTopic` (schema 1.8.67, line 10665).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForumTopicUpdate {
    pub chat_id: i64,
    pub forum_topic_id: i32,
    pub is_pinned: bool,
    pub last_read_inbox_message_id: i64,
    pub notification_settings: ChatNotificationSettings,
    pub unread_mention_count: i32,
    pub unread_reaction_count: i32,
}

fn icon_fields(info: &Value) -> (i32, i64) {
    let icon = info.get("icon");
    let color = icon
        .and_then(|i| i.get("color"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .sat_i32();
    let custom_emoji_id = int53_or_zero(icon.and_then(|i| i.get("custom_emoji_id")));
    (color, custom_emoji_id)
}

/// Subsection tabs: parse a `forumTopicInfo` object.
pub(crate) fn parse_forum_topic_info(info: &Value) -> Option<ForumTopicInfoUpdate> {
    let flag = |name: &str| info.get(name).and_then(Value::as_bool).unwrap_or(false);
    let (icon_color, icon_custom_emoji_id) = icon_fields(info);
    Some(ForumTopicInfoUpdate {
        chat_id: info.get("chat_id").and_then(Value::as_i64)?,
        forum_topic_id: info.get("forum_topic_id")?.as_i64()?.sat_i32(),
        name: json_field_str(info, "name"),
        icon_color,
        icon_custom_emoji_id,
        is_general: flag("is_general"),
        is_closed: flag("is_closed"),
        is_hidden: flag("is_hidden"),
    })
}

/// Subsection tabs: parse an `updateForumTopic` object.
pub(crate) fn parse_forum_topic_update(value: &Value) -> Option<ForumTopicUpdate> {
    Some(ForumTopicUpdate {
        chat_id: value.get("chat_id").and_then(Value::as_i64)?,
        forum_topic_id: value.get("forum_topic_id")?.as_i64()?.sat_i32(),
        is_pinned: value
            .get("is_pinned")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        last_read_inbox_message_id: int53_or_zero(value.get("last_read_inbox_message_id")),
        notification_settings: parse_chat_notification_settings(value.get("notification_settings")),
        unread_mention_count: int53_or_zero(value.get("unread_mention_count")).sat_i32(),
        unread_reaction_count: int53_or_zero(value.get("unread_reaction_count")).sat_i32(),
    })
}

/// Parse one `forumTopic` object. Returns `None` when `info` is missing or
/// malformed (the row is skipped, matching the lenient message parsing).
pub(crate) fn parse_forum_topic(value: &Value) -> Option<ForumTopic> {
    let info = value.get("info")?;
    let forum_topic_id = info.get("forum_topic_id")?.as_i64()?.sat_i32();
    let name = json_field_str(info, "name");
    let is_general = info
        .get("is_general")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_closed = info
        .get("is_closed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_hidden = info
        .get("is_hidden")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_pinned = value
        .get("is_pinned")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let unread_count = value
        .get("unread_count")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .sat_i32();
    let order = int53_or_zero(value.get("order"));
    let last_message = value
        .get("last_message")
        .and_then(|m| parse_message(m).ok());
    let last_message_id = last_message.as_ref().map_or(0, |m| m.id.0);
    let last_message_preview = last_message
        .map(|m| effective_content(&m.content, m.ephemeral.as_ref()).preview())
        .unwrap_or_default();
    let (icon_color, icon_custom_emoji_id) = icon_fields(info);
    Some(ForumTopic {
        forum_topic_id,
        name,
        is_general,
        is_closed,
        is_hidden,
        is_pinned,
        unread_count,
        order,
        last_message_preview,
        icon_color,
        icon_custom_emoji_id,
        last_message_id,
        last_read_inbox_message_id: int53_or_zero(value.get("last_read_inbox_message_id")),
        notification_settings: parse_chat_notification_settings(value.get("notification_settings")),
        unread_mention_count: int53_or_zero(value.get("unread_mention_count")).sat_i32(),
        unread_reaction_count: int53_or_zero(value.get("unread_reaction_count")).sat_i32(),
    })
}
