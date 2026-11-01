//! Reducer tests: a forum topic's window is capped at both ends like the
//! main history, and pages its newest end back in.
use super::common::*;
use super::*;

const CHAT: i64 = 16;
const TOPIC: i32 = 2;
const CAP: i64 = HISTORY_WINDOW_CAP as i64;

fn topic_session() -> (Session, Arc<MemorySink>, AtomicU64) {
    let (mut session, sink) = session();
    session.open_chat(ChatId(CHAT));
    session.select_topic(ChatId(CHAT), TOPIC);
    (session, sink, AtomicU64::new(0))
}

fn message(id: i64, outgoing: bool) -> String {
    format!(
        r#"{{"id":{id},"chat_id":{CHAT},"is_outgoing":{outgoing},"topic_id":{{"@type":"messageTopicForum","forum_topic_id":{TOPIC}}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"m","entities":[]}}}}}}"#
    )
}

fn page_request(session: &mut Session, purpose: RequestPurpose) -> RequestId {
    session.request_for_topic(purpose, Some(ChatId(CHAT)), TOPIC)
}

/// A `foundChatMessages` answer to `extra` with `ids`, newest first.
fn answer(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<MemorySink>,
    extra: RequestId,
    ids: &[i64],
    next_from: i64,
) {
    let messages: Vec<String> = ids.iter().map(|id| message(*id, false)).collect();
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":100000,"next_from_message_id":{next_from},"messages":[{}]}}"#,
            extra.0,
            messages.join(",")
        ),
    );
}

fn older_page(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<MemorySink>,
    ids: std::ops::RangeInclusive<i64>,
) {
    let extra = page_request(session, RequestPurpose::GetTopicHistory);
    let next_from = if *ids.start() <= 1 { 0 } else { *ids.start() };
    let ids: Vec<i64> = ids.rev().collect();
    answer(session, seq, sink, extra, &ids, next_from);
}

fn newer_page(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<MemorySink>,
    ids: std::ops::RangeInclusive<i64>,
) {
    let extra = page_request(session, RequestPurpose::GetTopicHistoryNewer);
    let ids: Vec<i64> = ids.rev().collect();
    answer(session, seq, sink, extra, &ids, 0);
}

fn live(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, id: i64) {
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{}}}"#,
            message(id, false)
        ),
    );
}

fn topic(session: &Session) -> &TopicHistory {
    &session.threads.topic_histories[&(CHAT, TOPIC)]
}

fn ids(session: &Session) -> (i64, i64) {
    let history = topic(session);
    let first = history.messages.keys().next().copied().unwrap_or(0);
    let last = history.newest_loaded_id().map_or(0, |id| id.0);
    (first, last)
}

/// A topic read back from its latest message (100_000) to `oldest`, a
/// page of 100 at a time.
fn read_back(oldest: i64) -> (Session, Arc<MemorySink>, AtomicU64) {
    let (mut session, sink, seq) = topic_session();
    let mut top = 100_000;
    while top >= oldest {
        let bottom = (top - 99).max(oldest);
        older_page(&mut session, &seq, &sink, bottom..=top);
        top = bottom - 1;
    }
    (session, sink, seq)
}

#[test]
fn reading_far_back_through_a_topic_keeps_its_window_bounded() {
    let (session, _, _) = read_back(100_000 - 3 * CAP);
    let history = topic(&session);
    assert!(history.messages.len() <= HISTORY_WINDOW_CAP);
    assert_eq!(ids(&session).0, 100_000 - 3 * CAP, "the reader's rows stay");
    assert!(history.has_newer, "the latest messages page back in");
    assert_eq!(history.latest_seen, 100_000);
}

#[test]
fn scrolling_back_down_pages_the_newest_end_in_and_drops_the_oldest() {
    let (mut session, sink, seq) = read_back(100_000 - 2 * CAP);
    let (oldest, mut newest) = ids(&session);
    while topic(&session).has_newer {
        newer_page(
            &mut session,
            &seq,
            &sink,
            newest..=(newest + 99).min(100_000),
        );
        let history = topic(&session);
        assert!(history.messages.len() <= HISTORY_WINDOW_CAP);
        newest = ids(&session).1;
    }
    assert_eq!(newest, 100_000, "back at the latest");
    let history = topic(&session);
    assert!(ids(&session).0 > oldest, "the far top went");
    assert!(!history.loaded_complete);
    assert_eq!(
        history.next_from_message_id,
        MessageId(ids(&session).0),
        "older pages continue from the oldest kept row"
    );
    // The window is one contiguous run.
    let keys: Vec<i64> = history.messages.keys().copied().collect();
    assert!(keys.windows(2).all(|pair| pair[1] == pair[0] + 1));
}

#[test]
fn a_live_message_past_a_window_short_of_the_latest_waits_outside_it() {
    let (mut session, sink, seq) = read_back(100_000 - 2 * CAP);
    let newest = ids(&session).1;
    live(&mut session, &seq, &sink, 100_001);
    assert_eq!(ids(&session).1, newest, "no row across the gap");
    assert_eq!(topic(&session).latest_seen, 100_001);
}

#[test]
fn an_own_send_in_a_window_short_of_the_latest_opens_the_latest_page() {
    let (mut session, sink, seq) = read_back(100_000 - 2 * CAP);
    let epoch = topic(&session).window_epoch;
    let send = session.request(RequestPurpose::SendMessage, Some(ChatId(CHAT)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"message","@extra":"{}","id":200000,"chat_id":{CHAT},"is_outgoing":true,"sending_state":{{"@type":"messageSendingStatePending"}},"topic_id":{{"@type":"messageTopicForum","forum_topic_id":{TOPIC}}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}"#,
            send.0
        ),
    );
    let history = topic(&session);
    assert_eq!(history.messages.len(), 1, "only the send stays");
    assert!(!history.has_newer);
    assert_eq!(
        history.next_from_message_id,
        MessageId(0),
        "the latest page"
    );
    assert_eq!(history.window_epoch, epoch + 1);
}

#[test]
fn a_topic_left_open_at_the_latest_stays_within_the_cap() {
    let (mut session, sink, seq) = topic_session();
    older_page(&mut session, &seq, &sink, 1..=CAP);
    for id in CAP + 1..=CAP + 400 {
        live(&mut session, &seq, &sink, id);
    }
    let history = topic(&session);
    assert!(history.messages.len() <= HISTORY_WINDOW_CAP);
    assert_eq!(ids(&session).1, CAP + 400, "the latest stays");
    assert!(!history.has_newer);
    assert!(!history.loaded_complete, "older rows page back in");
}

#[test]
fn a_reader_near_the_top_keeps_the_oldest_rows_and_the_latest_waits() {
    let (mut session, sink, seq) = topic_session();
    older_page(&mut session, &seq, &sink, 1..=CAP);
    assert!(topic(&session).loaded_complete);
    session.note_history_reader(
        (CHAT, Some(TOPIC), 0),
        ReaderPosition::Away { top: MessageId(50) },
    );
    live(&mut session, &seq, &sink, CAP + 1);
    let history = topic(&session);
    assert_eq!(ids(&session).0, 1, "the rows above stay");
    assert!(history.loaded_complete);
    assert!(history.has_newer, "the newest end went and pages back in");
    let newest = ids(&session).1;
    assert!(newest < CAP);
    live(&mut session, &seq, &sink, CAP + 2);
    assert_eq!(ids(&session).1, newest);
    assert_eq!(topic(&session).latest_seen, CAP + 2);
}

#[test]
fn a_newer_page_asked_for_before_the_newest_end_went_is_dropped() {
    let (mut session, sink, seq) = read_back(100_000 - CAP + 50);
    let newer = page_request(&mut session, RequestPurpose::GetTopicHistoryNewer);
    // The reader pages further back; the newest end goes.
    older_page(
        &mut session,
        &seq,
        &sink,
        100_000 - CAP - 99..=100_000 - CAP + 49,
    );
    let newest = ids(&session).1;
    assert!(newest < 100_000);
    answer(&mut session, &seq, &sink, newer, &[100_002, 100_001], 0);
    assert_eq!(ids(&session).1, newest, "no row across the gap");
    assert!(topic(&session).stale_pages.is_empty());
}

#[test]
fn jumping_to_the_latest_replaces_the_window_and_forgets_pages_on_the_way() {
    let (mut session, sink, seq) = read_back(100_000 - 2 * CAP);
    let older = page_request(&mut session, RequestPurpose::GetTopicHistory);
    assert!(session.reset_topic_window(ChatId(CHAT), TOPIC));
    assert!(topic(&session).messages.is_empty());
    assert!(
        !session.topic_page_in_flight(ChatId(CHAT), TOPIC, WindowEnd::Oldest),
        "the latest page can go out"
    );
    // The old page fails late: no Retry row for the new window.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"Timeout"}}"#,
            older.0
        ),
    );
    assert!(!session.topic_history_load_failed(ChatId(CHAT), TOPIC));
    older_page(&mut session, &seq, &sink, 99_901..=100_000);
    assert_eq!(ids(&session), (99_901, 100_000));
    assert!(!topic(&session).has_newer);
}

#[test]
fn closing_a_topic_short_of_the_latest_forgets_its_window() {
    let (mut session, _, _) = read_back(100_000 - 2 * CAP);
    let history = session
        .threads
        .topic_histories
        .get_mut(&(CHAT, TOPIC))
        .unwrap();
    history.trim_to_tail(50);
    assert!(history.messages.is_empty());
    assert!(!history.has_newer);
    assert_eq!(history.next_from_message_id, MessageId(0));
}
