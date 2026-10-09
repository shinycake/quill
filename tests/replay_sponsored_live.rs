//! Sponsored messages in live channel history: tail selection, view-once
//! accounting, refetch window, report/hide handling.
mod replay_common;
use replay_common::*;

#[test]
fn sponsored_tail_views_count_once() {
    let sink = Arc::new(MemorySink::new());
    let seq = AtomicU64::new(0);
    let mut session = sponsored_test_session(&sink, &seq);
    let chat_id = quill::ids::ChatId(13);

    // Fetch -> store: the first text ad is the one shown after the last message.
    assert_eq!(session.open_sponsored_tail().unwrap().message_id, 9001);
    // Visible -> view: counted once, however often it is reported.
    assert_eq!(session.take_sponsored_views(chat_id, &[9001]), vec![9001]);
    assert!(session.take_sponsored_views(chat_id, &[9001]).is_empty());
    // Unknown ids and other chats are ignored.
    assert!(session.take_sponsored_views(chat_id, &[1234]).is_empty());
    assert!(
        session
            .take_sponsored_views(quill::ids::ChatId(99), &[9001])
            .is_empty()
    );
    // A failed send releases the id for a retry.
    session.untake_sponsored_views(chat_id, &[9001]);
    assert_eq!(session.take_sponsored_views(chat_id, &[9001]), vec![9001]);

    // Within five minutes no refetch is due; after, one is.
    let now = std::time::Instant::now();
    assert!(!session.sponsored_fetch_due(chat_id, now));
    assert!(session.sponsored_fetch_due(chat_id, now + quill::state::SPONSORED_REFETCH_AFTER));
    assert!(session.sponsored_fetch_due(quill::ids::ChatId(77), now));
}

#[test]
fn sponsored_tail_hides_after_report_and_hide() {
    let sink = Arc::new(MemorySink::new());
    let seq = AtomicU64::new(0);
    let mut session = sponsored_test_session(&sink, &seq);
    let chat_id = quill::ids::ChatId(13);

    // Reporting the shown ad removes it; the next one needs its photo first.
    assert!(session.begin_sponsored_report(chat_id, 9001).is_some());
    let extra = session.request(RequestPurpose::ReportChatSponsoredMessage, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"reportSponsoredResultOk","@extra":"{}"}}"#,
            extra.0
        )],
    );
    assert!(session.open_sponsored_tail().is_none());

    // Hide ads without Premium: notice only, ads stay available.
    assert_eq!(session.begin_sponsored_hide(chat_id, 9002), Some(false));
    assert_eq!(
        session
            .last_sponsored_report
            .clone()
            .unwrap()
            .user_message(),
        "Hiding sponsored messages needs Telegram Premium"
    );
    assert!(!session.sponsored_hidden);

    // Premium: the driver sends toggleHasSponsoredMessagesEnabled(false);
    // `ok` hides everything.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateOption","name":"is_premium","value":{"@type":"optionValueBoolean","value":true}}"#,
        ],
    );
    assert_eq!(session.begin_sponsored_hide(chat_id, 9002), Some(true));
    let extra = session.request(
        RequestPurpose::ToggleHasSponsoredMessagesEnabled,
        Some(chat_id),
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0)],
    );
    assert!(session.sponsored_hidden);
    assert!(session.open_sponsored_tail().is_none());
    assert!(!session.sponsored_fetch_due(chat_id, std::time::Instant::now()));
}
