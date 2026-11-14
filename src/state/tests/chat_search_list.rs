//! The in-chat search hits list: shown for new results, folded on demand,
//! listed again by a new search (tdesktop `ComposeSearch::Inner::hideList`).
use super::common::*;
use super::*;

fn hit(id: i64) -> String {
    format!(
        r#"{{"id":{id},"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hello {id}","entities":[]}}}}}}"#
    )
}

/// Alice's chat with message 101 loaded and a "hello" search answered with
/// hits 101 and 90.
fn searched() -> Session {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"updateNewMessage","message":{}}}"#, hit(101)),
    );
    session.open_chat(ChatId(11));
    assert!(session.open_chat_search());
    session.search.chat_search.begin_query("hello");
    let extra = session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11));
    session.search.chat_search.searching(extra);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":0,"messages":[{},{}]}}"#,
            extra.0,
            hit(101),
            hit(90)
        ),
    );
    session
}

fn list(session: &Session) -> Option<ChatSearchList> {
    session.search.chat_search.open().map(|open| open.list)
}

#[test]
fn found_hits_list_under_the_bar() {
    let session = searched();
    assert_eq!(session.search.chat_search.hits().len(), 2);
    assert!(session.search.chat_search.list_shown());
}

#[test]
fn landing_on_the_first_hit_keeps_the_list() {
    let mut session = searched();
    let _ = session.begin_chat_search_jump(MessageId(101));
    assert!(session.search.chat_search.list_shown());
}

#[test]
fn folding_keeps_the_bar_and_its_results() {
    let mut session = searched();
    session.search.chat_search.fold_list();
    assert!(!session.search.chat_search.list_shown());
    assert!(session.search.chat_search.is_open());
    assert_eq!(session.search.chat_search.hits().len(), 2);
    assert_eq!(session.search.chat_search.position_label(), "1 of 2");
}

#[test]
fn toggling_unfolds_and_folds() {
    let mut session = searched();
    session.search.chat_search.fold_list();
    session.search.chat_search.toggle_list();
    assert!(session.search.chat_search.list_shown());
    session.search.chat_search.toggle_list();
    assert!(!session.search.chat_search.list_shown());
}

#[test]
fn a_new_query_lists_again() {
    let mut session = searched();
    session.search.chat_search.fold_list();
    session.search.chat_search.begin_query("hel");
    assert_eq!(list(&session), Some(ChatSearchList::Shown));
}

#[test]
fn reopening_the_search_lists_again() {
    let mut session = searched();
    session.search.chat_search.fold_list();
    session.search.chat_search.close();
    assert!(session.open_chat_search());
    assert_eq!(list(&session), Some(ChatSearchList::Shown));
}

#[test]
fn nothing_found_lists_nothing() {
    let mut session = searched();
    session.search.chat_search.clear_query();
    assert!(!session.search.chat_search.list_shown());
}
