use super::*;
use serde_json::Value;

/// Phase D3c: `chatEventAction` (TDLib 1.8.67, schema lines 7764–7928).
/// The 16 high-value constructors are typed below; every other
/// constructor maps to `Unsupported` carrying its constructor name, so
/// the UI renders an honest generic row instead of invented details.
/// (The schema has no `chatEventInviteLinkCreated` — link creation has
/// no event constructor in 1.8.67.)
#[derive(Debug, Clone, PartialEq)]
pub enum ChatEventAction {
    /// `chatEventMessageEdited` (line 7764). `text` is a short excerpt of
    /// the new message's text (empty for non-text content).
    MessageEdited { message_id: i64, text: String },
    /// `chatEventMessageDeleted` (line 7767).
    MessageDeleted { message_id: i64, text: String },
    /// `chatEventMessagePinned` (line 7770).
    MessagePinned { message_id: i64, text: String },
    /// `chatEventMessageUnpinned` (line 7773).
    MessageUnpinned { message_id: i64, text: String },
    /// `chatEventPollStopped` (line 7776) — a poll was stopped by the
    /// event's actor. `is_quiz` distinguishes TGX's
    /// `EventLogPollStopped` / `EventLogQuizStopped` copy; it degrades
    /// to `false` when the embedded `message` can't be parsed.
    PollStopped { is_quiz: bool },
    /// `chatEventMemberJoined` (line 7779).
    MemberJoined,
    /// `chatEventMemberJoinedByInviteLink` (line 7782).
    MemberJoinedByInviteLink {
        invite_link: String,
        invite_link_name: String,
    },
    /// `chatEventMemberJoinedByRequest` (line 7785).
    MemberJoinedByRequest {
        approver_user_id: i64,
        invite_link: String,
    },
    /// `chatEventMemberInvited` (line 7788). The invitee's resulting
    /// status is carried for honest phrasing ("invited"/"added").
    MemberInvited {
        user_id: i64,
        status: ChannelMemberStatus,
    },
    /// `chatEventMemberPromoted` (line 7794).
    MemberPromoted {
        user_id: i64,
        old_status: ChannelMemberStatus,
        new_status: ChannelMemberStatus,
    },
    /// `chatEventMemberRestricted` (line 7797). Covers restrictions,
    /// bans, and their reversals (old/new statuses distinguish them).
    MemberRestricted {
        member_id: MessageSender,
        old_status: ChannelMemberStatus,
        new_status: ChannelMemberStatus,
    },
    /// `chatEventDescriptionChanged` (line 7812).
    DescriptionChanged {
        old_description: String,
        new_description: String,
    },
    /// `chatEventPhotoChanged` (line 7830).
    PhotoChanged,
    /// `chatEventTitleChanged` (line 7842).
    TitleChanged {
        old_title: String,
        new_title: String,
    },
    /// `chatEventInviteLinkEdited` (line 7886).
    InviteLinkEdited {
        old_url: String,
        old_name: String,
        new_url: String,
        new_name: String,
    },
    /// `chatEventInviteLinkRevoked` (line 7889).
    InviteLinkRevoked { url: String, name: String },
    /// `chatEventInviteLinkDeleted` (line 7892).
    InviteLinkDeleted { url: String, name: String },
    /// Any other `chatEvent*` constructor — the schema defines 53 (lines
    /// 7764–7928); the remainder render as generic rows.
    Unsupported { type_name: String },
}

/// Phase D3c: `chatEvent` (TDLib 1.8.67, `schema/td_api.tl:7935`):
/// `chatEvent id:int64 date:int32 member_id:MessageSender action:ChatEventAction = ChatEvent;`
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedChatEvent {
    pub id: i64,
    pub date: i32,
    pub member_id: MessageSender,
    pub action: ChatEventAction,
}

/// Phase D3c: short text excerpt of a `message` object inside a
/// `chatEvent*` action (edited/deleted/pinned). Only `messageText`
/// content yields text; anything else is an empty string so the UI
/// falls back to an honest media-neutral phrasing.
pub(crate) fn chat_event_message_excerpt(message: Option<&Value>) -> (i64, String) {
    let message = match message {
        Some(message) => message,
        None => return (0, String::new()),
    };
    let id = int53(message.get("id")).unwrap_or(0);
    let raw = message
        .get("content")
        .filter(|content| content.get("@type").and_then(Value::as_str) == Some("messageText"))
        .and_then(|content| content.get("text"))
        .and_then(|text| text.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut excerpt: String = raw.chars().take(80).collect();
    if raw.chars().count() > 80 {
        excerpt.push('…');
    }
    (id, excerpt)
}

/// Phase D3c: `chatEventAction` object → typed action. The 17 handled
/// constructors (schema lines 7764/7767/7770/7773/7776/7779/7782/7785/
/// 7788/7794/7797/7812/7830/7842/7886/7889/7892) parse their fields; every
/// other constructor degrades to `ChatEventAction::Unsupported` with its
/// constructor name (never invented details).
pub(crate) fn parse_chat_event_action(value: Option<&Value>) -> ChatEventAction {
    let value = match value {
        Some(value) => value,
        None => {
            return ChatEventAction::Unsupported {
                type_name: String::new(),
            };
        }
    };
    let type_name = value
        .get("@type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let member_status = |value: Option<&Value>| {
        parse_channel_member_status(value)
            .map(|(status, _)| status)
            .unwrap_or(ChannelMemberStatus::Unknown)
    };
    let invite_link_url_name = |value: Option<&Value>| {
        parse_chat_invite_link(value)
            .map(|link| (link.invite_link, link.name))
            .unwrap_or_default()
    };
    let string_field = |value: &Value, field: &str| {
        value
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    match type_name.as_str() {
        "chatEventMessageEdited" => {
            let (message_id, text) = chat_event_message_excerpt(value.get("new_message"));
            ChatEventAction::MessageEdited { message_id, text }
        }
        "chatEventMessageDeleted" => {
            let (message_id, text) = chat_event_message_excerpt(value.get("message"));
            ChatEventAction::MessageDeleted { message_id, text }
        }
        "chatEventMessagePinned" => {
            let (message_id, text) = chat_event_message_excerpt(value.get("message"));
            ChatEventAction::MessagePinned { message_id, text }
        }
        "chatEventMessageUnpinned" => {
            let (message_id, text) = chat_event_message_excerpt(value.get("message"));
            ChatEventAction::MessageUnpinned { message_id, text }
        }
        "chatEventPollStopped" => {
            let poll = value
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(|content| content.get("poll"));
            let is_quiz = parse_poll(poll)
                .is_some_and(|poll| matches!(poll.poll_type, PollType::Quiz { .. }));
            ChatEventAction::PollStopped { is_quiz }
        }
        "chatEventMemberJoined" => ChatEventAction::MemberJoined,
        "chatEventMemberJoinedByInviteLink" => {
            let (invite_link, invite_link_name) = invite_link_url_name(value.get("invite_link"));
            ChatEventAction::MemberJoinedByInviteLink {
                invite_link,
                invite_link_name,
            }
        }
        "chatEventMemberJoinedByRequest" => {
            let (invite_link, _) = invite_link_url_name(value.get("invite_link"));
            ChatEventAction::MemberJoinedByRequest {
                approver_user_id: int53(value.get("approver_user_id")).unwrap_or(0),
                invite_link,
            }
        }
        "chatEventMemberInvited" => ChatEventAction::MemberInvited {
            user_id: int53(value.get("user_id")).unwrap_or(0),
            status: member_status(value.get("status")),
        },
        "chatEventMemberPromoted" => ChatEventAction::MemberPromoted {
            user_id: int53(value.get("user_id")).unwrap_or(0),
            old_status: member_status(value.get("old_status")),
            new_status: member_status(value.get("new_status")),
        },
        "chatEventMemberRestricted" => ChatEventAction::MemberRestricted {
            member_id: parse_message_sender(value.get("member_id"))
                .unwrap_or(MessageSender::User { user_id: 0 }),
            old_status: member_status(value.get("old_status")),
            new_status: member_status(value.get("new_status")),
        },
        "chatEventDescriptionChanged" => ChatEventAction::DescriptionChanged {
            old_description: string_field(value, "old_description"),
            new_description: string_field(value, "new_description"),
        },
        "chatEventPhotoChanged" => ChatEventAction::PhotoChanged,
        "chatEventTitleChanged" => ChatEventAction::TitleChanged {
            old_title: string_field(value, "old_title"),
            new_title: string_field(value, "new_title"),
        },
        "chatEventInviteLinkEdited" => {
            let (old_url, old_name) = invite_link_url_name(value.get("old_invite_link"));
            let (new_url, new_name) = invite_link_url_name(value.get("new_invite_link"));
            ChatEventAction::InviteLinkEdited {
                old_url,
                old_name,
                new_url,
                new_name,
            }
        }
        "chatEventInviteLinkRevoked" => {
            let (url, name) = invite_link_url_name(value.get("invite_link"));
            ChatEventAction::InviteLinkRevoked { url, name }
        }
        "chatEventInviteLinkDeleted" => {
            let (url, name) = invite_link_url_name(value.get("invite_link"));
            ChatEventAction::InviteLinkDeleted { url, name }
        }
        _ => ChatEventAction::Unsupported { type_name },
    }
}

/// Phase D3c: `chatEvent` (schema 1.8.67, line 7935). Events whose
/// `member_id` (the actor) fails to parse are dropped rather than
/// misattributed; `id` and `date` are required.
pub(crate) fn parse_chat_event(value: &Value) -> Option<ParsedChatEvent> {
    Some(ParsedChatEvent {
        id: int53(value.get("id")).ok()?,
        date: int53(value.get("date")).ok()? as i32,
        member_id: parse_message_sender(value.get("member_id")).ok()?,
        action: parse_chat_event_action(value.get("action")),
    })
}
