use super::*;
use serde_json::Value;

/// `SavedMessagesTopicType` (TDLib 1.8.67, `schema/td_api.tl:3903-3909`):
/// where the messages of a Saved Messages sublist came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SavedTopicKind {
    /// Messages saved without a source ("My Notes").
    MyNotes,
    /// Forwarded from a user who hides their account ("Author Hidden").
    AuthorHidden,
    /// Forwarded from this chat.
    FromChat(i64),
}

/// One `savedMessagesTopic` (schema line 3919). The draft stays out: the
/// sublist view has no composer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedMessagesTopic {
    pub id: i64,
    pub kind: SavedTopicKind,
    pub is_pinned: bool,
    /// Topics sort by `order` descending.
    pub order: i64,
    pub last_message_id: i64,
    pub last_message_date: i32,
    /// `MessageContent::preview` of `last_message`; empty when none.
    pub last_message_preview: String,
    /// The hidden sender's name from the last message's forward origin
    /// (`messageOriginHiddenUser`), when TDLib sent one.
    pub hidden_sender_name: String,
}

/// One `savedMessagesTag` (schema line 3561).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedMessagesTag {
    pub tag: ReactionType,
    /// 0-12 characters; empty when the tag has no name.
    pub label: String,
    pub count: i32,
}

pub(crate) fn parse_saved_messages_topic(value: &Value) -> Option<SavedMessagesTopic> {
    let id = int53(value.get("id")).ok()?;
    let kind = match value
        .get("type")
        .and_then(|t| t.get("@type"))
        .and_then(Value::as_str)?
    {
        "savedMessagesTopicTypeMyNotes" => SavedTopicKind::MyNotes,
        "savedMessagesTopicTypeAuthorHidden" => SavedTopicKind::AuthorHidden,
        "savedMessagesTopicTypeSavedFromChat" => {
            SavedTopicKind::FromChat(int53(value.get("type")?.get("chat_id")).ok()?)
        }
        _ => return None,
    };
    let last_message = value
        .get("last_message")
        .and_then(|m| parse_message(m).ok());
    let hidden_sender_name = value
        .get("last_message")
        .and_then(|m| m.get("forward_info"))
        .and_then(|f| f.get("origin"))
        .filter(|o| o.get("@type").and_then(Value::as_str) == Some("messageOriginHiddenUser"))
        .map(|o| json_field_str(o, "sender_name"))
        .unwrap_or_default();
    Some(SavedMessagesTopic {
        id,
        kind,
        is_pinned: value
            .get("is_pinned")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        order: int53_or_zero(value.get("order")),
        last_message_id: last_message.as_ref().map_or(0, |m| m.id.0),
        last_message_date: last_message.as_ref().map_or(0, |m| m.date),
        last_message_preview: last_message
            .map(|m| effective_content(&m.content, m.ephemeral.as_ref()).preview())
            .unwrap_or_default(),
        hidden_sender_name,
    })
}

pub(crate) fn parse_saved_messages_tags(value: &Value) -> Vec<SavedMessagesTag> {
    value
        .get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tag| {
            Some(SavedMessagesTag {
                tag: parse_reaction_type(tag.get("tag"))?,
                label: json_field_str(tag, "label"),
                count: int53_or_zero(tag.get("count")).sat_i32(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_types_and_preview_parse() {
        let json = r#"{"@type":"updateSavedMessagesTopic","topic":{"@type":"savedMessagesTopic","id":"-1001","type":{"@type":"savedMessagesTopicTypeSavedFromChat","chat_id":-1001},"is_pinned":true,"order":"500","last_message":{"id":3145728,"chat_id":13,"date":1700000000,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Remember this","entities":[]}}},"draft_message":null}}"#;
        let value: Value = serde_json::from_str(json).unwrap();
        let topic = parse_saved_messages_topic(&value["topic"]).unwrap();
        assert_eq!(topic.id, -1001);
        assert_eq!(topic.kind, SavedTopicKind::FromChat(-1001));
        assert!(topic.is_pinned);
        assert_eq!(topic.order, 500);
        assert_eq!(topic.last_message_id, 3145728);
        assert_eq!(topic.last_message_date, 1700000000);
        assert_eq!(topic.last_message_preview, "Remember this");
    }

    #[test]
    fn my_notes_author_hidden_and_unknown() {
        let notes: Value = serde_json::from_str(
            r#"{"id":"13","type":{"@type":"savedMessagesTopicTypeMyNotes"},"order":"1"}"#,
        )
        .unwrap();
        assert_eq!(
            parse_saved_messages_topic(&notes).unwrap().kind,
            SavedTopicKind::MyNotes
        );
        let hidden: Value = serde_json::from_str(
            r#"{"id":"0","type":{"@type":"savedMessagesTopicTypeAuthorHidden"},"order":"2","last_message":{"id":5,"chat_id":13,"date":1,"is_outgoing":true,"forward_info":{"origin":{"@type":"messageOriginHiddenUser","sender_name":"Ghost"}},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"x","entities":[]}}}}"#,
        )
        .unwrap();
        let parsed = parse_saved_messages_topic(&hidden).unwrap();
        assert_eq!(parsed.kind, SavedTopicKind::AuthorHidden);
        assert_eq!(parsed.hidden_sender_name, "Ghost");
        let unknown: Value =
            serde_json::from_str(r#"{"id":"1","type":{"@type":"savedMessagesTopicTypeNew"}}"#)
                .unwrap();
        assert!(parse_saved_messages_topic(&unknown).is_none());
    }

    #[test]
    fn tags_parse_with_labels() {
        let value: Value = serde_json::from_str(
            r#"{"@type":"savedMessagesTags","tags":[{"tag":{"@type":"reactionTypeEmoji","emoji":"❤"},"label":"Love","count":3},{"tag":{"@type":"reactionTypeCustomEmoji","custom_emoji_id":"77"},"label":"","count":1},{"tag":{"@type":"reactionTypePaid"},"label":"","count":9}]}"#,
        )
        .unwrap();
        let tags = parse_saved_messages_tags(&value);
        assert_eq!(tags.len(), 3);
        assert_eq!(tags[0].tag, ReactionType::emoji("\u{2764}"));
        assert_eq!(tags[0].label, "Love");
        assert_eq!(tags[0].count, 3);
        assert_eq!(
            tags[1].tag,
            ReactionType::CustomEmoji {
                custom_emoji_id: 77
            }
        );
    }
}
