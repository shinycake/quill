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
        session.unread_totals.main.messages,
        Some(UnreadPair {
            all: 44,
            unmuted: 13
        })
    );
    assert_eq!(
        session.unread_totals.main.chats,
        Some(UnreadPair {
            all: 20,
            unmuted: 6
        })
    );
    assert_eq!(
        session.unread_totals.archive.messages,
        Some(UnreadPair {
            all: 30,
            unmuted: 0
        })
    );
    assert_eq!(session.unread_totals.archive.chats, None);
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
