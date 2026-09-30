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
}

/// Parse one `forumTopic` object. Returns `None` when `info` is missing or
/// malformed (the row is skipped, matching the lenient message parsing).
pub(crate) fn parse_forum_topic(value: &Value) -> Option<ForumTopic> {
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
        .unwrap_or(0) as i32;
    let order = int53_or_zero(value.get("order"));
    let last_message_preview = value
        .get("last_message")
        .and_then(|m| parse_message(m).ok())
        .map(|m| effective_content(&m.content, m.ephemeral.as_ref()).preview())
        .unwrap_or_default();
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
    })
}
