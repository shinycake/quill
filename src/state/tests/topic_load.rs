//! Reducer tests: the load state of a forum topic's first history page
//! (skeleton while it loads, Retry row when it fails, empty only once the
//! empty page arrived).
use super::common::*;
use super::*;

fn topic_session() -> (Session, Arc<MemorySink>, AtomicU64) {
    let (mut session, sink) = session();
    session.open_chat(ChatId(16));
    session.select_topic(ChatId(16), 2);
    (session, sink, AtomicU64::new(0))
}

fn answer_error(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, extra: RequestId) {
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"Timeout"}}"#,
            extra.0
        ),
    );
}

#[test]
fn a_topic_without_an_entry_is_loading_not_empty() {
    let (session, _, _) = topic_session();
    assert!(session.topic_history_loading(ChatId(16), 2));
    assert!(!session.topic_history_load_failed(ChatId(16), 2));
}

#[test]
fn a_failed_first_page_shows_the_retry_state() {
    let (mut session, sink, seq) = topic_session();
    let extra = session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 2);
    assert!(session.topic_history_loading(ChatId(16), 2));
    answer_error(&mut session, &seq, &sink, extra);
    assert!(session.topic_history_load_failed(ChatId(16), 2));
    assert!(!session.topic_history_loading(ChatId(16), 2));
    // Another topic of the same chat is unaffected.
    assert!(!session.topic_history_load_failed(ChatId(16), 3));
}

#[test]
fn a_timed_out_first_page_shows_the_retry_state() {
    let (mut session, _, _) = topic_session();
    session.connection = ConnectionState::Ready;
    session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 2);
    let later = std::time::Instant::now() + std::time::Duration::from_secs(600);
    session.sweep_stale_requests(later, std::time::Duration::from_secs(30), |_| false);
    assert!(session.topic_history_load_failed(ChatId(16), 2));
    assert!(!session.topic_history_loading(ChatId(16), 2));
}

#[test]
fn an_empty_answer_settles_into_the_empty_topic() {
    let (mut session, sink, seq) = topic_session();
    let extra = session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 2);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":0,"next_from_message_id":0,"messages":[]}}"#,
            extra.0
        ),
    );
    assert!(!session.topic_history_loading(ChatId(16), 2));
    assert!(!session.topic_history_load_failed(ChatId(16), 2));
}

#[test]
fn a_landed_page_clears_an_earlier_failure() {
    let (mut session, sink, seq) = topic_session();
    let first = session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 2);
    answer_error(&mut session, &seq, &sink, first);
    let retry = session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 2);
    session.clear_topic_load_failed(ChatId(16), 2);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[{{"id":50,"chat_id":16,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"x","entities":[]}}}}}}]}}"#,
            retry.0
        ),
    );
    assert!(!session.topic_history_load_failed(ChatId(16), 2));
    assert!(!session.topic_history_loading(ChatId(16), 2));
}

#[test]
fn opening_the_topic_again_forgets_the_failure() {
    let (mut session, sink, seq) = topic_session();
    let extra = session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 2);
    answer_error(&mut session, &seq, &sink, extra);
    assert!(session.topic_history_load_failed(ChatId(16), 2));
    session.select_topic(ChatId(16), 2);
    assert!(!session.topic_history_load_failed(ChatId(16), 2));
}
