//! State reducer tests: OS notification rules (events, alerts, folder counters).
use super::common::*;
use super::*;

const ADA_CHAT: &str = r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#;
const PINNED: &str = r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":false,"content":{"@type":"messagePinMessage","message_id":5}}}"#;
const JOINED: &str = r#"{"@type":"updateNewMessage","message":{"id":61,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageContactRegistered"}}}"#;

#[test]
fn contact_joined_option_gates_the_notification() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, ADA_CHAT);
    session.app_active = false;
    apply_json(&mut session, &seq, &sink, JOINED);
    assert_eq!(session.settings.pending_notifications.len(), 1);
    session.settings.pending_notifications.clear();
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateOption","name":"disable_contact_registered_notifications","value":{"@type":"optionValueBoolean","value":true}}"#,
    );
    assert!(session.settings.disable_contact_registered_notifications);
    apply_json(&mut session, &seq, &sink, JOINED);
    assert!(session.settings.pending_notifications.is_empty());
}

#[test]
fn pinned_scope_default_gates_the_notification() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, ADA_CHAT);
    session.app_active = false;
    apply_json(&mut session, &seq, &sink, PINNED);
    assert_eq!(session.settings.pending_notifications.len(), 1);
    session.settings.pending_notifications.clear();
    // The private-chat scope turns pinned-message notifications off.
    session.settings.scope_notification_settings.insert(
        NotificationSettingsScope::PrivateChats,
        ScopeNotificationSettings {
            disable_pinned_message_notifications: true,
            ..ScopeNotificationSettings::default()
        },
    );
    apply_json(&mut session, &seq, &sink, PINNED);
    assert!(session.settings.pending_notifications.is_empty());
    // Ordinary messages still notify.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":62,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
    );
    assert_eq!(session.settings.pending_notifications.len(), 1);
}

#[test]
fn attention_follows_the_flash_pref_not_the_desktop_switch() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, ADA_CHAT);
    session.app_active = false;
    session.settings.desktop_notifications = false;
    apply_json(&mut session, &seq, &sink, JOINED);
    assert!(session.settings.pending_notifications.is_empty());
    assert!(session.settings.pending_attention);
    session.settings.pending_attention = false;
    session.settings.badge_prefs.flash_bounce = false;
    apply_json(&mut session, &seq, &sink, JOINED);
    assert!(!session.settings.pending_attention);
}

#[test]
fn a_focused_open_chat_does_not_ask_for_attention() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, ADA_CHAT);
    session.app_active = true;
    session.open_chat = Some(ChatId(7));
    apply_json(&mut session, &seq, &sink, JOINED);
    assert!(!session.settings.pending_attention);
}

#[test]
fn folder_chat_counts_feed_the_tab_badge() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUnreadChatCount","chat_list":{"@type":"chatListFolder","chat_folder_id":5},"total_count":9,"unread_count":4,"unread_unmuted_count":1,"marked_as_unread_count":0,"marked_as_unread_unmuted_count":0}"#,
    );
    let pair = session.chat_list.folder_unread_chats[&5];
    assert_eq!(
        pair.folder_badge(true),
        Some(FolderBadge {
            count: 4,
            muted: false
        })
    );
    assert_eq!(
        pair.folder_badge(false),
        Some(FolderBadge {
            count: 1,
            muted: false
        })
    );
}

#[test]
fn folder_badge_is_muted_when_only_muted_chats_are_unread() {
    let only_muted = UnreadPair { all: 3, unmuted: 0 };
    assert_eq!(
        only_muted.folder_badge(true),
        Some(FolderBadge {
            count: 3,
            muted: true
        })
    );
    // Excluding muted chats hides the badge entirely.
    assert_eq!(only_muted.folder_badge(false), None);
    assert_eq!(UnreadPair::default().folder_badge(true), None);
}
