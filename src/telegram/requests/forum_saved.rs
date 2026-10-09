//! Forum extras and Saved Messages sublists / tags (batch B16).
//!
//! Every constructor below is quoted from `schema/td_api.tl` (TDLib 1.8.67);
//! the unit tests compare the fields with the schema line.

use crate::ids::{ChatId, RequestId};
use serde_json::{Value, json};

/// The six topic icon colors tdesktop offers (`ForumTopicIcons`,
/// `data_forum_topic.cpp`): blue, yellow, violet, green, rose, red.
pub const TOPIC_ICON_COLORS: [i32; 6] = [0x6FB9F0, 0xFFD67E, 0xCB86DB, 0x8EEE98, 0xFF93B2, 0xFB6F5F];

/// Whether `color` is one of [`TOPIC_ICON_COLORS`].
pub fn valid_topic_icon_color(color: i32) -> bool {
    TOPIC_ICON_COLORS.contains(&color)
}

/// `toggleChatViewAsTopics chat_id:int53 view_as_topics:Bool = Ok;`
/// ("Changes the view_as_topics setting of a forum chat or Saved Messages").
pub fn toggle_chat_view_as_topics(
    extra: RequestId,
    chat_id: ChatId,
    view_as_topics: bool,
) -> String {
    json!({
        "@type": "toggleChatViewAsTopics",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "view_as_topics": view_as_topics,
    })
    .to_string()
}

/// `getForumTopicDefaultIcons = Stickers;` — the custom emoji any user may
/// use as a forum topic icon.
pub fn get_forum_topic_default_icons(extra: RequestId) -> String {
    json!({
        "@type": "getForumTopicDefaultIcons",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `getForumTopicLink chat_id:int53 forum_topic_id:int32 = MessageLink;`
/// (an offline method).
pub fn get_forum_topic_link(extra: RequestId, chat_id: ChatId, forum_topic_id: i32) -> String {
    json!({
        "@type": "getForumTopicLink",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "forum_topic_id": forum_topic_id,
    })
    .to_string()
}

/// `setPinnedForumTopics chat_id:int53 forum_topic_ids:vector<int32> = Ok;`
pub fn set_pinned_forum_topics(extra: RequestId, chat_id: ChatId, topic_ids: &[i32]) -> String {
    json!({
        "@type": "setPinnedForumTopics",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "forum_topic_ids": topic_ids,
    })
    .to_string()
}

/// `readAllForumTopicMentions chat_id:int53 forum_topic_id:int32 = Ok;`
pub fn read_all_forum_topic_mentions(
    extra: RequestId,
    chat_id: ChatId,
    forum_topic_id: i32,
) -> String {
    json!({
        "@type": "readAllForumTopicMentions",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "forum_topic_id": forum_topic_id,
    })
    .to_string()
}

/// `readAllForumTopicReactions chat_id:int53 forum_topic_id:int32 = Ok;`
pub fn read_all_forum_topic_reactions(
    extra: RequestId,
    chat_id: ChatId,
    forum_topic_id: i32,
) -> String {
    json!({
        "@type": "readAllForumTopicReactions",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "forum_topic_id": forum_topic_id,
    })
    .to_string()
}

/// `unpinAllForumTopicMessages chat_id:int53 forum_topic_id:int32 = Ok;`
pub fn unpin_all_forum_topic_messages(
    extra: RequestId,
    chat_id: ChatId,
    forum_topic_id: i32,
) -> String {
    json!({
        "@type": "unpinAllForumTopicMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "forum_topic_id": forum_topic_id,
    })
    .to_string()
}

/// `createForumTopic chat_id:int53 name:string is_name_implicit:Bool
/// icon:forumTopicIcon = ForumTopicInfo;` with a chosen icon: `color` is
/// an RGB value, `custom_emoji_id` 0 keeps the plain colored icon.
pub fn create_forum_topic_with_icon(
    extra: RequestId,
    chat_id: ChatId,
    name: &str,
    color: i32,
    custom_emoji_id: i64,
) -> String {
    json!({
        "@type": "createForumTopic",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "name": name,
        "is_name_implicit": false,
        "icon": {
            "@type": "forumTopicIcon",
            "color": color,
            "custom_emoji_id": custom_emoji_id,
        },
    })
    .to_string()
}

/// `editForumTopic chat_id:int53 forum_topic_id:int32 name:string
/// edit_icon_custom_emoji:Bool icon_custom_emoji_id:int64 = Ok;` changing
/// the icon as well (0 removes the custom emoji).
pub fn edit_forum_topic_with_icon(
    extra: RequestId,
    chat_id: ChatId,
    forum_topic_id: i32,
    name: &str,
    icon_custom_emoji_id: i64,
) -> String {
    json!({
        "@type": "editForumTopic",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "forum_topic_id": forum_topic_id,
        "name": name,
        "edit_icon_custom_emoji": true,
        "icon_custom_emoji_id": icon_custom_emoji_id,
    })
    .to_string()
}

/// `loadSavedMessagesTopics limit:int32 = Ok;` — the topics arrive through
/// `updateSavedMessagesTopic`; a 404 means all were loaded.
pub fn load_saved_messages_topics(extra: RequestId, limit: i32) -> String {
    json!({
        "@type": "loadSavedMessagesTopics",
        "@extra": extra.as_extra(),
        "limit": limit,
    })
    .to_string()
}

/// `getSavedMessagesTopicHistory saved_messages_topic_id:int53
/// from_message_id:int53 offset:int32 limit:int32 = Messages;`
pub fn get_saved_messages_topic_history(
    extra: RequestId,
    saved_messages_topic_id: i64,
    from_message_id: i64,
    offset: i32,
    limit: i32,
) -> String {
    json!({
        "@type": "getSavedMessagesTopicHistory",
        "@extra": extra.as_extra(),
        "saved_messages_topic_id": saved_messages_topic_id,
        "from_message_id": from_message_id,
        "offset": offset,
        "limit": limit,
    })
    .to_string()
}

/// `deleteSavedMessagesTopicHistory saved_messages_topic_id:int53 = Ok;`
pub fn delete_saved_messages_topic_history(extra: RequestId, saved_messages_topic_id: i64) -> String {
    json!({
        "@type": "deleteSavedMessagesTopicHistory",
        "@extra": extra.as_extra(),
        "saved_messages_topic_id": saved_messages_topic_id,
    })
    .to_string()
}

/// `toggleSavedMessagesTopicIsPinned saved_messages_topic_id:int53
/// is_pinned:Bool = Ok;`
pub fn toggle_saved_messages_topic_pinned(
    extra: RequestId,
    saved_messages_topic_id: i64,
    is_pinned: bool,
) -> String {
    json!({
        "@type": "toggleSavedMessagesTopicIsPinned",
        "@extra": extra.as_extra(),
        "saved_messages_topic_id": saved_messages_topic_id,
        "is_pinned": is_pinned,
    })
    .to_string()
}

/// `getSavedMessagesTags saved_messages_topic_id:int53 = SavedMessagesTags;`
/// (0 returns the tags of all Saved Messages).
pub fn get_saved_messages_tags(extra: RequestId, saved_messages_topic_id: i64) -> String {
    json!({
        "@type": "getSavedMessagesTags",
        "@extra": extra.as_extra(),
        "saved_messages_topic_id": saved_messages_topic_id,
    })
    .to_string()
}

/// `setSavedMessagesTagLabel tag:ReactionType label:string = Ok;` (Premium;
/// the label is 0-12 characters).
pub fn set_saved_messages_tag_label(extra: RequestId, tag: &Value, label: &str) -> String {
    json!({
        "@type": "setSavedMessagesTagLabel",
        "@extra": extra.as_extra(),
        "tag": tag,
        "label": label,
    })
    .to_string()
}

/// `searchSavedMessages saved_messages_topic_id:int53 tag:ReactionType
/// query:string from_message_id:int53 offset:int32 limit:int32 =
/// FoundChatMessages;` — `tag` null searches without a tag filter.
pub fn search_saved_messages(
    extra: RequestId,
    saved_messages_topic_id: i64,
    tag: Option<&Value>,
    query: &str,
    from_message_id: i64,
    limit: i32,
) -> String {
    json!({
        "@type": "searchSavedMessages",
        "@extra": extra.as_extra(),
        "saved_messages_topic_id": saved_messages_topic_id,
        "tag": tag.cloned().unwrap_or(Value::Null),
        "query": query,
        "from_message_id": from_message_id,
        "offset": 0,
        "limit": limit,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema_line(name: &str) -> String {
        include_str!("../../../schema/td_api.tl")
            .lines()
            .find(|l| l.starts_with(&format!("{name} ")))
            .unwrap_or_else(|| panic!("{name} missing from the schema"))
            .to_owned()
    }

    fn parse(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    /// Every field the builder sends exists as a `name:` in the schema line.
    fn assert_fields_in_schema(json: &str) {
        let v = parse(json);
        let name = v["@type"].as_str().unwrap().to_owned();
        let line = schema_line(&name);
        for key in v.as_object().unwrap().keys() {
            if key == "@type" || key == "@extra" {
                continue;
            }
            assert!(
                line.contains(&format!("{key}:")),
                "{name}: `{key}` is not in `{line}`"
            );
        }
    }

    #[test]
    fn forum_builders_match_the_schema() {
        let r = RequestId(5);
        let c = ChatId(-100);
        for json in [
            toggle_chat_view_as_topics(r, c, false),
            get_forum_topic_link(r, c, 3),
            set_pinned_forum_topics(r, c, &[3, 1]),
            read_all_forum_topic_mentions(r, c, 3),
            read_all_forum_topic_reactions(r, c, 3),
            unpin_all_forum_topic_messages(r, c, 3),
            create_forum_topic_with_icon(r, c, "Ideas", 0xFFD67E, 99),
            edit_forum_topic_with_icon(r, c, 3, "Ideas", 0),
            get_forum_topic_default_icons(r),
        ] {
            assert_fields_in_schema(&json);
        }
        let v = parse(&set_pinned_forum_topics(r, c, &[3, 1]));
        assert_eq!(v["forum_topic_ids"], json!([3, 1]));
        let v = parse(&create_forum_topic_with_icon(r, c, "Ideas", 0xFFD67E, 99));
        assert_eq!(v["icon"]["@type"], "forumTopicIcon");
        assert_eq!(v["icon"]["color"], 0xFFD67E);
        assert_eq!(v["icon"]["custom_emoji_id"], 99);
        let v = parse(&edit_forum_topic_with_icon(r, c, 3, "Ideas", 0));
        assert_eq!(v["edit_icon_custom_emoji"], true);
        assert_eq!(v["icon_custom_emoji_id"], 0);
    }

    #[test]
    fn topic_colors_are_the_six_defaults() {
        assert!(valid_topic_icon_color(0x6FB9F0));
        assert!(valid_topic_icon_color(0xFB6F5F));
        assert!(!valid_topic_icon_color(0x123456));
    }

    #[test]
    fn saved_builders_match_the_schema() {
        let r = RequestId(8);
        let tag = json!({"@type": "reactionTypeEmoji", "emoji": "\u{2764}"});
        for json in [
            load_saved_messages_topics(r, 50),
            get_saved_messages_topic_history(r, 7, 0, 0, 40),
            delete_saved_messages_topic_history(r, 7),
            toggle_saved_messages_topic_pinned(r, 7, true),
            get_saved_messages_tags(r, 0),
            set_saved_messages_tag_label(r, &tag, "Work"),
            search_saved_messages(r, 0, Some(&tag), "", 0, 50),
            search_saved_messages(r, 7, None, "hi", 0, 50),
        ] {
            assert_fields_in_schema(&json);
        }
        let v = parse(&search_saved_messages(r, 0, None, "", 0, 50));
        assert!(v["tag"].is_null());
        let v = parse(&set_saved_messages_tag_label(r, &tag, "Work"));
        assert_eq!(v["tag"]["emoji"], "\u{2764}");
        assert_eq!(v["label"], "Work");
    }
}
