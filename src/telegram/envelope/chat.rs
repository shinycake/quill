use super::*;
use crate::ids::{ChatId, MessageId, UserId};
use serde_json::Value;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatPositionUpdate {
    pub chat_id: ChatId,
    pub list: ChatList,
    pub order: i64,
    pub is_pinned: bool,
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

/// Slice CL2: parse a bare chat object (the createPrivateChat
/// answer, schema 1.8.67 line 13312) exactly like the inner chat of
/// updateNewChat, so the reducer inserts it into the model through
/// the existing path.
pub(crate) fn parse_new_chat(chat: &Value) -> Result<EnvelopePayload, ParseError> {
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
        notification_settings: parse_chat_notification_settings(chat.get("notification_settings")),
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
        // Slice G1: full `chatPermissions` block for the editor.
        permissions: parse_chat_permissions(chat.get("permissions")),
        // Slice G1: delete gate for `deleteChat` (schema 1.8.67,
        // lines 3616/11848).
        can_be_deleted_for_all_users: chat
            .get("can_be_deleted_for_all_users")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        // Slice CL1: clear-history gate for `deleteChatHistory`
        // (schema 1.8.67, lines 3616/11845).
        can_be_deleted_only_for_self: chat
            .get("can_be_deleted_only_for_self")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        // `chat.has_protected_content` (schema 1.8.67, line 3598).
        has_protected_content: chat
            .get("has_protected_content")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        has_scheduled_messages: chat
            .get("has_scheduled_messages")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        // `chat.is_translatable` (schema 1.8.67, line 3599).
        is_translatable: chat
            .get("is_translatable")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        // `chat.view_as_topics` (schema 1.8.67, line 3627): a forum shown
        // as topics, or Saved Messages shown as chats. Absent means unset.
        view_as_topics: chat.get("view_as_topics").and_then(Value::as_bool),
        // Slice CL1: `chat.is_marked_as_unread` (schema 1.8.67,
        // lines 3600/3627).
        is_marked_as_unread: chat
            .get("is_marked_as_unread")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        // Phase B4: `chat.message_auto_delete_time` (schema 1.8.67,
        // lines 3616 / 3627). Defaults to 0 (disabled) when
        // absent — the field is new enough that older TDLib
        // builds may omit it.
        message_auto_delete_time: chat
            .get("message_auto_delete_time")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        // Phase C3a: `chat.video_chat` (`videoChat`, schema
        // 1.8.67, lines 3576 / 3579). `group_call_id` 0 → None
        // (no active video chat).
        video_chat: parse_video_chat(chat.get("video_chat")).filter(|v| v.group_call_id != 0),
        // Slice G2: `chat.has_welcome_messages` (schema 1.8.67,
        // lines 3603/3627).
        has_welcome_messages: chat
            .get("has_welcome_messages")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        // Slice CL3: mention / reaction badge counts (schema 1.8.67,
        // lines 3611-3612/3627). Default 0 — older TDLib builds may
        // omit them.
        unread_mention_count: chat
            .get("unread_mention_count")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        unread_reaction_count: chat
            .get("unread_reaction_count")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        // Slice CL3: `chat.can_be_reported` (schema 1.8.67, lines
        // 3606/3627) gates the row-menu Report item.
        can_be_reported: chat
            .get("can_be_reported")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        // Batch 8: `chat.action_bar` (schema 1.8.67, line 3627).
        action_bar: parse_chat_action_bar(chat.get("action_bar")),
        // Slice CL3: `chat.block_list` (schema 1.8.67, lines 3627/9692);
        // `blockListMain` means the peer is blocked.
        blocked: is_block_list_main(chat.get("block_list")),
        positions: parse_position_list(ChatId(int53(chat.get("id"))?), chat.get("positions")),
        last_message: match chat.get("last_message") {
            None | Some(Value::Null) => None,
            Some(message) => super::parse_message(message).ok().map(Box::new),
        },
    })
}

pub(crate) fn parse_chat_kind(value: Option<&Value>) -> ChatKind {
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

pub(crate) fn parse_position(value: &Value) -> Result<ChatPositionUpdate, ParseError> {
    let chat_id = ChatId(int53(value.get("chat_id"))?);
    let position = value.get("position").unwrap_or(value);
    Ok(parse_position_entry(chat_id, position))
}

pub(crate) fn parse_position_entry(chat_id: ChatId, position: &Value) -> ChatPositionUpdate {
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

pub(crate) fn parse_position_list(
    chat_id: ChatId,
    value: Option<&Value>,
) -> Vec<ChatPositionUpdate> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|position| parse_position_entry(chat_id, position))
        .collect()
}
