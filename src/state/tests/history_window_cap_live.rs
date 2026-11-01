//! State reducer tests: live messages in chats nobody has open are capped
//! like page loads.
use super::common::session;
use super::{
    ChatId, FileId, HISTORY_WINDOW_CAP, HISTORY_WINDOW_TRIM_TO, MessageContent, MessageId,
    ParsedFile, ParsedMessage, Session, WindowEnd, history_message,
};
use crate::telegram::LocalFileState;

fn file(id: i32) -> ParsedFile {
    ParsedFile {
        id: FileId(id),
        size: 10,
        expected_size: 10,
        uploading: false,
        local: LocalFileState {
            path: String::new(),
            can_be_downloaded: true,
            is_downloading_active: false,
            is_downloading_completed: false,
            downloaded_size: 0,
        },
    }
}

fn live(chat: i64, id: i64, files: Vec<ParsedFile>) -> ParsedMessage {
    ParsedMessage {
        sender: None,
        id: MessageId(id),
        chat_id: ChatId(chat),
        date: 0,
        is_outgoing: false,
        is_pinned: false,
        topic_id: None,
        thread_id: None,
        ephemeral: None,
        media_album_id: 0,
        author_signature: None,
        scheduling_state: None,
        can_retry: false,
        send_state: Default::default(),
        content: MessageContent::Text("m".into()),
        files,
        reply_to: None,
        forward_info: None,
        extras: Default::default(),
        interaction_info: None,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
    }
}

fn feed(session: &mut Session, chat: i64, count: i64) {
    for id in 1..=count {
        session.upsert_message(live(chat, id, vec![file(1)]), false);
    }
}

#[test]
fn a_chat_nobody_has_open_stays_within_the_cap() {
    let (mut session, _sink) = session();
    let total = HISTORY_WINDOW_CAP as i64 + 10;
    feed(&mut session, 7, total);
    let history = &session.histories[&7];
    assert!(history.messages.len() <= HISTORY_WINDOW_CAP);
    assert_eq!(history.newest_id(), Some(MessageId(total)), "newest stay");
    assert!(!history.loaded_complete, "older ones page back in");
}

fn feed_live(session: &mut Session, chat: i64, ids: std::ops::RangeInclusive<i64>) {
    for id in ids {
        session.apply_update_new_message(live(chat, id, Vec::new()));
    }
}

#[test]
fn a_page_landing_in_a_shown_chat_is_not_trimmed_row_by_row() {
    let (mut session, _sink) = session();
    session.open_chat(ChatId(7));
    let total = HISTORY_WINDOW_CAP as i64 + 10;
    feed(&mut session, 7, total);
    assert_eq!(session.histories[&7].messages.len() as i64, total);
}

#[test]
fn a_chat_left_open_at_the_tail_stays_within_the_cap() {
    let (mut session, _sink) = session();
    session.open_chat(ChatId(7));
    let total = HISTORY_WINDOW_CAP as i64 + 400;
    feed_live(&mut session, 7, 1..=total);
    let history = &session.histories[&7];
    assert!(history.messages.len() <= HISTORY_WINDOW_CAP);
    assert_eq!(history.newest_id(), Some(MessageId(total)));
    assert!(!history.loaded_complete, "older rows page back in");
    assert!(!session.history_has_newer(ChatId(7)), "still at the latest");
}

#[test]
fn a_reader_in_the_middle_keeps_their_rows_while_old_ones_go() {
    let (mut session, _sink) = session();
    session.open_chat(ChatId(7));
    let cap = HISTORY_WINDOW_CAP as i64;
    feed_live(&mut session, 7, 1..=cap);
    session.note_history_reader((7, None, 0), Some(1_000));
    feed_live(&mut session, 7, cap + 1..=cap + 1);
    let history = &session.histories[&7];
    assert_eq!(history.messages.len(), HISTORY_WINDOW_TRIM_TO);
    assert!(
        history.oldest_id() < Some(MessageId(800)),
        "rows above stay"
    );
    assert_eq!(history.newest_id(), Some(MessageId(cap + 1)));
    assert!(!session.history_has_newer(ChatId(7)));
}

#[test]
fn a_reader_near_the_top_keeps_the_oldest_rows_and_the_tail_waits() {
    let (mut session, _sink) = session();
    session.open_chat(ChatId(7));
    let cap = HISTORY_WINDOW_CAP as i64;
    feed_live(&mut session, 7, 1..=cap);
    session.histories.get_mut(&7).unwrap().loaded_complete = true;
    session.note_history_reader((7, None, 0), Some(50));
    feed_live(&mut session, 7, cap + 1..=cap + 1);
    assert_eq!(session.histories[&7].oldest_id(), Some(MessageId(1)));
    assert!(session.histories[&7].loaded_complete);
    assert!(session.history_has_newer(ChatId(7)), "paged back in below");
    feed_live(&mut session, 7, cap + 2..=cap + 500);
    let history = &session.histories[&7];
    assert_eq!(history.messages.len(), HISTORY_WINDOW_TRIM_TO);
    assert_eq!(history.latest_seen, cap + 500);
}

#[test]
fn a_trim_at_the_tail_never_takes_a_send_in_flight() {
    let (mut session, _sink) = session();
    session.open_chat(ChatId(7));
    let cap = HISTORY_WINDOW_CAP as i64;
    feed_live(&mut session, 7, 1..=cap);
    let mut sending = live(7, cap + 1, Vec::new());
    sending.is_outgoing = true;
    session.upsert_message(sending, true);
    session.note_history_reader((7, None, 0), Some(1));
    feed_live(&mut session, 7, cap + 2..=cap + 2);
    let history = &session.histories[&7];
    assert!(history.contains(MessageId(cap + 1)));
    assert_eq!(history.messages.len() as i64, cap + 2, "tried again later");
}

#[test]
fn a_reopened_chat_starts_at_the_tail_again() {
    let (mut session, _sink) = session();
    session.open_chat(ChatId(7));
    session.note_history_reader((7, None, 0), Some(5));
    session.open_chat(ChatId(8));
    session.open_chat(ChatId(7));
    assert_eq!(session.histories[&7].reader_top, None);
}

#[test]
fn a_forum_topic_on_screen_drops_only_rows_above_its_reader() {
    let (mut session, _sink) = session();
    let cap = HISTORY_WINDOW_CAP as i64;
    session.topic_history_entry(ChatId(7), 3);
    for id in 1..=cap + 1 {
        let mut message = live(7, id, Vec::new());
        message.topic_id = Some(3);
        session.apply_update_new_message(message);
    }
    let topic = &session.threads.topic_histories[&(7, 3)];
    assert_eq!(topic.messages.len(), HISTORY_WINDOW_TRIM_TO, "at the tail");
    assert_eq!(topic.messages.keys().last(), Some(&(cap + 1)));
    assert_eq!(topic.next_from_message_id, MessageId(cap + 2 - 1200));

    session.note_history_reader((7, Some(3), 0), Some(cap + 2 - 1200 + 5));
    for id in cap + 2..=cap + 400 {
        let mut message = live(7, id, Vec::new());
        message.topic_id = Some(3);
        session.apply_update_new_message(message);
    }
    let topic = &session.threads.topic_histories[&(7, 3)];
    assert_eq!(
        topic.messages.len(),
        1200 + 399,
        "the reader keeps their rows"
    );
}
