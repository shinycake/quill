//! Rows of the topic and reply-thread info cards (tdesktop shows the same
//! facts in the topic profile and the replies section header). Pure
//! formatting; names and dates come in already resolved.

use crate::telegram::envelope::ForumTopic;

/// One label/value line of an info card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoRow {
    pub label: &'static str,
    pub value: String,
}

fn row(label: &'static str, value: impl Into<String>) -> InfoRow {
    InfoRow {
        label,
        value: value.into(),
    }
}

fn plural(count: i32, one: &str, other: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { other })
}

/// Info rows for a forum topic. `creator` is the resolved name of
/// `forumTopicInfo.creator_id` and `created` its formatted creation date.
pub fn topic_info_rows(
    topic: &ForumTopic,
    creator: Option<&str>,
    created: Option<&str>,
) -> Vec<InfoRow> {
    let mut rows = Vec::new();
    if topic.is_outgoing {
        rows.push(row("Created by", "You"));
    } else if let Some(name) = creator.filter(|name| !name.is_empty()) {
        rows.push(row("Created by", name));
    }
    if let Some(date) = created.filter(|date| !date.is_empty()) {
        rows.push(row("Created", date));
    }
    let mut status = Vec::new();
    if topic.is_general {
        status.push("General");
    }
    if topic.is_pinned {
        status.push("Pinned");
    }
    if topic.is_closed {
        status.push("Closed");
    }
    if topic.is_hidden {
        status.push("Hidden");
    }
    if !status.is_empty() {
        rows.push(row("Status", status.join(", ")));
    }
    if topic.unread_count > 0 {
        rows.push(row(
            "Unread",
            plural(topic.unread_count, "message", "messages"),
        ));
    }
    if topic.unread_mention_count > 0 {
        rows.push(row(
            "Mentions",
            plural(topic.unread_mention_count, "mention", "mentions"),
        ));
    }
    rows
}

/// Info rows for a reply thread. `comments` is true for a channel post's
/// discussion; `started_by` and `started` describe the root message.
pub fn thread_info_rows(
    comments: bool,
    reply_count: i32,
    unread_count: i32,
    started_by: Option<&str>,
    started: Option<&str>,
) -> Vec<InfoRow> {
    let mut rows = Vec::new();
    let (one, other) = if comments {
        ("comment", "comments")
    } else {
        ("reply", "replies")
    };
    rows.push(row(
        if comments { "Comments" } else { "Replies" },
        plural(reply_count.max(0), one, other),
    ));
    if unread_count > 0 {
        rows.push(row("Unread", plural(unread_count, "reply", "replies")));
    }
    if let Some(name) = started_by.filter(|name| !name.is_empty()) {
        rows.push(row(if comments { "Posted by" } else { "Started by" }, name));
    }
    if let Some(date) = started.filter(|date| !date.is_empty()) {
        rows.push(row(if comments { "Posted" } else { "Started" }, date));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::{InfoRow, thread_info_rows, topic_info_rows};
    use crate::telegram::envelope::{ChatNotificationSettings, ForumTopic};

    fn topic() -> ForumTopic {
        ForumTopic {
            forum_topic_id: 3,
            name: "Releases".into(),
            is_general: false,
            is_closed: false,
            is_pinned: false,
            is_hidden: false,
            unread_count: 0,
            order: 0,
            last_message_preview: String::new(),
            icon_color: 0,
            icon_custom_emoji_id: 0,
            last_message_id: 0,
            last_read_inbox_message_id: 0,
            notification_settings: ChatNotificationSettings::default(),
            unread_mention_count: 0,
            unread_reaction_count: 0,
            creation_date: 0,
            creator: None,
            is_outgoing: false,
        }
    }

    fn values(rows: &[InfoRow]) -> Vec<(&'static str, &str)> {
        rows.iter().map(|r| (r.label, r.value.as_str())).collect()
    }

    #[test]
    fn plain_topic_without_data_has_no_rows() {
        assert!(topic_info_rows(&topic(), None, None).is_empty());
    }

    #[test]
    fn topic_rows_cover_creator_date_status_and_counts() {
        let mut t = topic();
        t.is_pinned = true;
        t.is_closed = true;
        t.unread_count = 1;
        t.unread_mention_count = 2;
        let rows = topic_info_rows(&t, Some("Dana"), Some("3 March 2026, 10:00"));
        assert_eq!(
            values(&rows),
            vec![
                ("Created by", "Dana"),
                ("Created", "3 March 2026, 10:00"),
                ("Status", "Pinned, Closed"),
                ("Unread", "1 message"),
                ("Mentions", "2 mentions"),
            ]
        );
        t.is_outgoing = true;
        assert_eq!(
            topic_info_rows(&t, Some("Dana"), None)[0].value,
            "You",
            "own topics say You even when a name is known"
        );
    }

    #[test]
    fn thread_rows_word_comments_and_replies() {
        let rows = thread_info_rows(true, 1, 0, Some("News"), Some("1 May 2026, 09:00"));
        assert_eq!(
            values(&rows),
            vec![
                ("Comments", "1 comment"),
                ("Posted by", "News"),
                ("Posted", "1 May 2026, 09:00"),
            ]
        );
        let rows = thread_info_rows(false, 5, 2, None, None);
        assert_eq!(
            values(&rows),
            vec![("Replies", "5 replies"), ("Unread", "2 replies")]
        );
    }
}
