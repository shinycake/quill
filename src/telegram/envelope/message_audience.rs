//! Who saw, listened to or reacted to a message: `messageViewers`,
//! `messageReadDate` and `addedReactions` (TDLib 1.8.67, schema lines
//! 2859-2879 and 7315-7318). They feed the message menu's "N Seen" /
//! "N Reacted" row and its list.

use super::*;
use serde_json::Value;

/// One `messageViewer`: a chat member who viewed the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageViewer {
    pub user_id: i64,
    /// Approximate Unix time the message was viewed.
    pub view_date: i32,
}

/// `MessageReadDate`: when the other side of a private chat read your
/// message, or why that is unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageReadDate {
    Read(i32),
    Unread,
    TooOld,
    UserPrivacyRestricted,
    MyPrivacyRestricted,
}

/// One `addedReaction`: who reacted with what, and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddedReaction {
    pub reaction_type: ReactionType,
    pub sender: MessageSender,
    pub is_outgoing: bool,
    pub date: i32,
}

/// One page of `addedReactions`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddedReactionsPage {
    pub total_count: i32,
    pub reactions: Vec<AddedReaction>,
    /// Empty when there is no next page.
    pub next_offset: String,
}

pub(crate) fn parse_message_viewers(value: &Value) -> Vec<MessageViewer> {
    value
        .get("viewers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|viewer| {
            Some(MessageViewer {
                user_id: int53(viewer.get("user_id")).ok()?,
                view_date: viewer
                    .get("view_date")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
            })
        })
        .collect()
}

pub(crate) fn parse_message_read_date(value: &Value) -> Option<MessageReadDate> {
    Some(match value.get("@type").and_then(Value::as_str)? {
        "messageReadDateRead" => MessageReadDate::Read(
            value.get("read_date").and_then(Value::as_i64).unwrap_or(0) as i32,
        ),
        "messageReadDateUnread" => MessageReadDate::Unread,
        "messageReadDateTooOld" => MessageReadDate::TooOld,
        "messageReadDateUserPrivacyRestricted" => MessageReadDate::UserPrivacyRestricted,
        "messageReadDateMyPrivacyRestricted" => MessageReadDate::MyPrivacyRestricted,
        _ => return None,
    })
}

pub(crate) fn parse_added_reactions(value: &Value) -> AddedReactionsPage {
    let reactions = value
        .get("reactions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some(AddedReaction {
                reaction_type: parse_reaction_type(entry.get("type"))?,
                sender: parse_message_sender(entry.get("sender_id")).ok()?,
                is_outgoing: entry
                    .get("is_outgoing")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                date: entry.get("date").and_then(Value::as_i64).unwrap_or(0) as i32,
            })
        })
        .collect::<Vec<_>>();
    AddedReactionsPage {
        total_count: value
            .get("total_count")
            .and_then(Value::as_i64)
            .unwrap_or(reactions.len() as i64) as i32,
        reactions,
        next_offset: value
            .get("next_offset")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    }
}
