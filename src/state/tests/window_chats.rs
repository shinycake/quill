//! State reducer tests: chats shown in windows of their own.
use super::common::*;
use super::*;

const CHAT_JSON: &str = r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#;

fn message_json(id: i64) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":7,"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"m{id}","entities":[]}}}}}}}}"#
    )
}

#[test]
fn a_chat_in_an_active_window_does_not_notify_but_an_inactive_one_does() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, CHAT_JSON);
    // The main window shows another chat and is not in front.
    session.app_active = false;
    session.open_chat = None;
    session.open_window_chat(ChatId(7));

    // Its window is the active one: the message is read on arrival
    // (Telegram Desktop reads what the active window shows).
    session.active_window_chats.insert(ChatId(7));
    apply_json(&mut session, &seq, &sink, &message_json(10));
    assert!(session.settings.pending_notifications.is_empty());

    // The window went behind another app: the next message notifies.
    session.active_window_chats.remove(&ChatId(7));
    apply_json(&mut session, &seq, &sink, &message_json(11));
    assert_eq!(session.settings.pending_notifications.len(), 1);
}

#[test]
fn the_main_window_rule_is_unchanged_by_window_chats() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, CHAT_JSON);
    // Open in the main window, which is in front: no notification.
    session.open_chat(ChatId(7));
    session.app_active = true;
    apply_json(&mut session, &seq, &sink, &message_json(10));
    assert!(session.settings.pending_notifications.is_empty());
    // The main window went to the background: it notifies.
    session.app_active = false;
    apply_json(&mut session, &seq, &sink, &message_json(11));
    assert_eq!(session.settings.pending_notifications.len(), 1);
}

#[test]
fn the_session_view_swap_is_symmetric() {
    let (mut session, _sink) = session();
    session.open_chat = Some(ChatId(1));
    session.open_topic = Some(5);
    let mut view = SessionView::of_chat(ChatId(2));
    session.swap_view(&mut view);
    assert_eq!(session.open_chat, Some(ChatId(2)));
    assert_eq!(session.open_topic, None);
    assert_eq!(view.open_chat, Some(ChatId(1)));
    assert_eq!(view.open_topic, Some(5));
    session.swap_view(&mut view);
    assert_eq!(session.open_chat, Some(ChatId(1)));
    assert_eq!(session.open_topic, Some(5));
    assert_eq!(view.open_chat, Some(ChatId(2)));
}
