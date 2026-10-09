//! State reducer tests: the R6 history window cap, driven through the real
//! `messages` page reducers.
use super::common::{MemorySink, apply_json, session};
use super::{
    ChatId, HISTORY_WINDOW_CAP, HISTORY_WINDOW_TRIM_TO, MessageId, RequestPurpose, Session,
    WindowEnd,
};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const CHAT: i64 = 5;

fn page_json(extra: u64, ids: impl Iterator<Item = i64>) -> String {
    let rows: Vec<String> = ids
        .map(|id| {
            format!(
                r#"{{"id":{id},"chat_id":{CHAT},"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"m{id}","entities":[]}}}}}}"#
            )
        })
        .collect();
    format!(
        r#"{{"@type":"messages","@extra":"{extra}","messages":[{}]}}"#,
        rows.join(",")
    )
}

fn land_older(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, count: i64) {
    let from = session.histories[&CHAT].oldest_id().map_or(0, |id| id.0);
    let extra =
        session.request_for_message(RequestPurpose::GetHistory, ChatId(CHAT), MessageId(from));
    // TDLib answers newest first.
    apply_json(
        session,
        seq,
        sink,
        &page_json(extra.0, (from - count..from).rev()),
    );
}

fn land_newer(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, count: i64) {
    let from = session.histories[&CHAT].newest_id().map_or(0, |id| id.0);
    let extra = session.request_for_message(
        RequestPurpose::GetHistoryNewer,
        ChatId(CHAT),
        MessageId(from),
    );
    apply_json(
        session,
        seq,
        sink,
        &page_json(extra.0, (from + 1..=from + count).rev()),
    );
}

/// Chat with its latest message 5000, opened at the tail (4951..=5000).
fn opened_at_tail() -> (Session, AtomicU64, Arc<MemorySink>) {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(CHAT));
    let extra = session.request_for_message(RequestPurpose::GetHistory, ChatId(CHAT), MessageId(0));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &page_json(extra.0, (4951..=5000).rev()),
    );
    (session, seq, sink)
}

#[test]
fn paging_back_far_trims_the_newest_end_and_marks_has_newer() {
    let (mut session, seq, sink) = opened_at_tail();
    assert!(!session.histories[&CHAT].has_newer);
    let epoch = session.histories[&CHAT].window_epoch;

    // Page back until the cap is crossed.
    while !session.histories[&CHAT].has_newer {
        assert!(
            session.histories[&CHAT].messages.len() <= HISTORY_WINDOW_CAP,
            "the cap must trim as soon as it is crossed"
        );
        land_older(&mut session, &seq, &sink, 50);
    }
    let history = &session.histories[&CHAT];
    assert_eq!(history.messages.len(), HISTORY_WINDOW_TRIM_TO);
    assert!(history.has_newer, "newest end trimmed => newer pages exist");
    assert!(!history.loaded_complete);
    // The reader's end (oldest) is intact; the window is contiguous.
    let oldest = history.oldest_id().unwrap().0;
    let newest = history.newest_id().unwrap().0;
    assert_eq!(newest - oldest + 1, HISTORY_WINDOW_TRIM_TO as i64);
    assert!(newest < 5000);
    // Trimming does not re-anchor the scroller.
    assert_eq!(history.window_epoch, epoch);
    // The tail is remembered, so has_newer survives later page refreshes.
    assert_eq!(history.latest_seen, 5000);

    // Another older page keeps it bounded and keeps has_newer.
    land_older(&mut session, &seq, &sink, 50);
    let history = &session.histories[&CHAT];
    assert_eq!(history.messages.len(), HISTORY_WINDOW_TRIM_TO + 50);
    assert!(history.has_newer);
}

#[test]
fn paging_forward_after_a_trim_pages_the_tail_back_in_and_trims_the_old_end() {
    let (mut session, seq, sink) = opened_at_tail();
    while !session.histories[&CHAT].has_newer {
        land_older(&mut session, &seq, &sink, 50);
    }
    let trimmed_newest = session.histories[&CHAT].newest_id().unwrap().0;

    // Newer pages continue exactly where the window now ends.
    land_newer(&mut session, &seq, &sink, 50);
    let history = &session.histories[&CHAT];
    assert_eq!(history.newest_id().unwrap().0, trimmed_newest + 50);
    assert!(history.has_newer, "still short of message 5000");

    // Read forward until the cap is crossed again: the oldest end goes.
    let oldest_before = history.oldest_id().unwrap().0;
    while session.histories[&CHAT].oldest_id().unwrap().0 == oldest_before {
        land_newer(&mut session, &seq, &sink, 50);
    }
    let history = &session.histories[&CHAT];
    assert_eq!(history.messages.len(), HISTORY_WINDOW_TRIM_TO);
    assert!(history.oldest_id().unwrap().0 > oldest_before);
    assert!(!history.loaded_complete, "older pages exist again");
}

#[test]
fn reaching_the_latest_message_clears_has_newer_and_stays_bounded() {
    let (mut session, seq, sink) = opened_at_tail();
    while !session.histories[&CHAT].has_newer {
        land_older(&mut session, &seq, &sink, 50);
    }
    // Page forward until the window reaches message 5000 again.
    while session.histories[&CHAT].has_newer {
        land_newer(&mut session, &seq, &sink, 50);
        assert!(session.histories[&CHAT].messages.len() <= HISTORY_WINDOW_CAP + 50);
    }
    let history = &session.histories[&CHAT];
    assert_eq!(history.newest_id(), Some(MessageId(5000)));
    assert!(!history.has_newer);
}

#[test]
fn jumping_back_to_latest_refetches_after_a_trim() {
    let (mut session, seq, sink) = opened_at_tail();
    while !session.histories[&CHAT].has_newer {
        land_older(&mut session, &seq, &sink, 50);
    }
    let epoch = session.histories[&CHAT].window_epoch;
    // The jump-to-latest path replaces the window (and re-anchors).
    session.reset_history_window(ChatId(CHAT));
    let history = &session.histories[&CHAT];
    assert!(history.messages.is_empty());
    assert!(!history.has_newer);
    assert_eq!(history.window_epoch, epoch + 1);
    // The refetched latest page lands as a normal tail window.
    let extra = session.request_for_message(RequestPurpose::GetHistory, ChatId(CHAT), MessageId(0));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &page_json(extra.0, (4951..=5000).rev()),
    );
    let history = &session.histories[&CHAT];
    assert_eq!(history.newest_id(), Some(MessageId(5000)));
    assert!(!history.has_newer);
}

#[test]
fn the_cap_constants_leave_headroom_between_trims() {
    const { assert!(HISTORY_WINDOW_TRIM_TO + 100 < HISTORY_WINDOW_CAP) };
    assert_ne!(WindowEnd::Oldest, WindowEnd::Newest);
}
