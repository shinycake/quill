//! State reducer tests: TDLib unread totals feeding the badge.
use super::common::*;
use super::*;
use crate::settings::BadgePrefs;
use crate::tray::badge_count;

// Recorded shapes of TDLib 1.8.67 `updateUnreadMessageCount` /
// `updateUnreadChatCount` (schema lines 10877 / 10886).
const MAIN_MESSAGES: &str = r#"{"@type":"updateUnreadMessageCount","chat_list":{"@type":"chatListMain"},"unread_count":44,"unread_unmuted_count":13}"#;
const MAIN_CHATS: &str = r#"{"@type":"updateUnreadChatCount","chat_list":{"@type":"chatListMain"},"total_count":312,"unread_count":20,"unread_unmuted_count":6,"marked_as_unread_count":2,"marked_as_unread_unmuted_count":1}"#;
const ARCHIVE_MESSAGES: &str = r#"{"@type":"updateUnreadMessageCount","chat_list":{"@type":"chatListArchive"},"unread_count":30,"unread_unmuted_count":0}"#;
const FOLDER_MESSAGES: &str = r#"{"@type":"updateUnreadMessageCount","chat_list":{"@type":"chatListFolder","chat_folder_id":5},"unread_count":999,"unread_unmuted_count":999}"#;

#[test]
fn unread_totals_are_stored_per_list() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for json in [MAIN_MESSAGES, MAIN_CHATS, ARCHIVE_MESSAGES, FOLDER_MESSAGES] {
        apply_json(&mut session, &seq, &sink, json);
    }
    assert_eq!(
        session.chat_list.unread_totals.main.messages,
        Some(UnreadPair {
            all: 44,
            unmuted: 13
        })
    );
    assert_eq!(
        session.chat_list.unread_totals.main.chats,
        Some(UnreadPair {
            all: 20,
            unmuted: 6
        })
    );
    assert_eq!(
        session.chat_list.unread_totals.archive.messages,
        Some(UnreadPair {
            all: 30,
            unmuted: 0
        })
    );
    assert_eq!(session.chat_list.unread_totals.archive.chats, None);
}

#[test]
fn badge_follows_totals_and_prefs() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    // One loaded chat with 1 unread: the pre-update sum.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"m","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":1}}"#,
    );
    assert_eq!(badge_count(&session, &BadgePrefs::default()), 1);
    for json in [MAIN_MESSAGES, MAIN_CHATS, ARCHIVE_MESSAGES] {
        apply_json(&mut session, &seq, &sink, json);
    }
    assert_eq!(badge_count(&session, &BadgePrefs::default()), 74);
    let unmuted_only = BadgePrefs {
        include_muted: false,
        ..BadgePrefs::default()
    };
    assert_eq!(badge_count(&session, &unmuted_only), 13);
    let chats = BadgePrefs {
        count_messages: false,
        include_archived: false,
        ..BadgePrefs::default()
    };
    assert_eq!(badge_count(&session, &chats), 20);
    // A later read lowers the total.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUnreadMessageCount","chat_list":{"@type":"chatListMain"},"unread_count":40,"unread_unmuted_count":9}"#,
    );
    assert_eq!(badge_count(&session, &unmuted_only), 9);
}

/// Live trace (Hermesio bot with topics): TDLib's main totals said 4 while
/// Telegram Desktop showed 16 = 4 + 11 + 1 from two topics with a read
/// position; two never-read topics (read position 0, bogus 16 and 3) and a
/// zero topic add nothing.
#[test]
fn badge_adds_forum_topic_unreads_on_top_of_totals() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":41,"first_name":"Bot","type":{"@type":"userTypeBot","has_topics":true,"allows_users_to_create_topics":false,"is_inline":false}}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":41,"title":"Bot","type":{"@type":"chatTypePrivate","user_id":41},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUnreadMessageCount","chat_list":{"@type":"chatListMain"},"unread_count":4,"unread_unmuted_count":1}"#,
    );
    assert_eq!(badge_count(&session, &BadgePrefs::default()), 4);
    let topic = |id: i32, unread: i32, read: i64, last: i64| {
        format!(
            r#"{{"info":{{"@type":"forumTopicInfo","chat_id":41,"forum_topic_id":{id},"name":"T{id}","icon":{{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"}},"is_general":false,"is_closed":false,"is_hidden":false}},"last_message":{{"id":{last},"chat_id":41,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"x","entities":[]}}}}}},"order":"{id}","is_pinned":false,"unread_count":{unread},"last_read_inbox_message_id":{read}}}"#,
        )
    };
    let extra = session.request(RequestPurpose::GetForumTopics, Some(ChatId(41)));
    let topics = [
        topic(482317, 11, 505806848000, 505859276800),
        topic(477080, 1, 500306018304, 505744982016),
        topic(481359, 16, 0, 504759320576),
        topic(479296, 3, 0, 502580379648),
        topic(470000, 0, 505000000000, 505000000000),
    ]
    .join(",");
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"forumTopics","@extra":"{}","total_count":5,"topics":[{topics}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
            extra.0
        ),
    );
    assert_eq!(badge_count(&session, &BadgePrefs::default()), 16);
    // Chats mode counts the forum chat once.
    let chats = BadgePrefs {
        count_messages: false,
        ..BadgePrefs::default()
    };
    session.chat_list.unread_totals.main.chats = Some(UnreadPair { all: 3, unmuted: 3 });
    assert_eq!(badge_count(&session, &chats), 4);
}
