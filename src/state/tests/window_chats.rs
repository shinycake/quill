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

const BOB_JSON: &str = r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":0}}"#;

/// A `searchChatMessages` answer with one hit in chat 7.
fn found_json(extra: RequestId) -> String {
    format!(
        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[{{"id":10,"chat_id":7,"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"m10","entities":[]}}}}}}]}}"#,
        extra.0
    )
}

/// Search chat 7 in its window's view, as typing in that window's search
/// field does. Returns the request and the search's generation.
fn search_in_window(session: &mut Session) -> (RequestId, u64) {
    session
        .with_view(Some(ChatId(7)), |session| {
            assert!(session.open_chat_search());
            let generation = session.search.chat_search.begin_query("m");
            let extra = session.request_chat_search(
                RequestPurpose::SearchChatMessages,
                ChatId(7),
                generation,
            );
            (extra, generation)
        })
        .expect("chat 7 has a window")
}

#[test]
fn a_search_answer_lands_in_the_window_that_asked() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, CHAT_JSON);
    apply_json(&mut session, &seq, &sink, BOB_JSON);
    session.open_chat(ChatId(8));
    session.open_window_chat(ChatId(7));
    let (extra, _) = search_in_window(&mut session);

    // The answer arrives with the main window's view in place (the poll
    // loop applies updates there).
    assert_eq!(session.view_in_place(), None);
    apply_json(&mut session, &seq, &sink, &found_json(extra));
    assert!(
        !session.search.chat_search.open,
        "the main window's search stayed shut"
    );
    assert!(session.search.chat_search.hits.is_empty());
    let hits = session.with_view(Some(ChatId(7)), |session| {
        (
            session.search.chat_search.status,
            session.search.chat_search.hits.len(),
        )
    });
    assert_eq!(
        hits,
        Some((SearchStatus::Ready, 1)),
        "the window got its hits"
    );
}

#[test]
fn a_search_answer_for_a_closed_window_lands_in_no_view() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, CHAT_JSON);
    session.open_window_chat(ChatId(7));
    let (extra, generation) = search_in_window(&mut session);
    // The window closes before the answer; the main window then opens the
    // same chat and searches it. Each view counts its own searches, so the
    // two can carry the same generation.
    assert!(session.close_window_chat(ChatId(7)));
    session.open_chat(ChatId(7));
    assert!(session.open_chat_search());
    session.search.chat_search.generation = generation - 1;
    assert_eq!(session.search.chat_search.begin_query("other"), generation);

    apply_json(&mut session, &seq, &sink, &found_json(extra));
    assert!(
        session.search.chat_search.hits.is_empty(),
        "the closed window's hits did not land in the main window's search"
    );
    assert_eq!(session.search.chat_search.status, SearchStatus::Searching);
}

#[test]
fn a_view_is_put_in_place_by_window_and_given_back() {
    let (mut session, _sink) = session();
    session.open_chat = Some(ChatId(1));
    session.open_topic = Some(5);
    session.open_window_chat(ChatId(2));
    assert!(session.enter_view(Some(ChatId(2))));
    assert_eq!(session.view_in_place(), Some(ChatId(2)));
    assert_eq!(
        (session.open_chat, session.open_topic),
        (Some(ChatId(2)), None)
    );
    // Requests registered meanwhile are the window's.
    let extra = session.request(RequestPurpose::GetChatPreview, Some(ChatId(2)));
    assert!(session.enter_view(None));
    assert_eq!(
        (session.open_chat, session.open_topic),
        (Some(ChatId(1)), Some(5))
    );
    assert_eq!(
        session
            .requests
            .take(extra)
            .and_then(|pending| pending.window),
        Some(ChatId(2))
    );
    // A chat with no window has no view to enter.
    assert!(!session.enter_view(Some(ChatId(3))));
    assert_eq!(session.view_in_place(), None);
    assert_eq!(session.with_view(Some(ChatId(3)), |_| ()), None);
    // Closing the window while its view is in place puts the main one back.
    assert!(session.enter_view(Some(ChatId(2))));
    assert!(session.close_window_chat(ChatId(2)));
    assert_eq!(session.view_in_place(), None);
    assert_eq!(session.open_chat, Some(ChatId(1)));
}
