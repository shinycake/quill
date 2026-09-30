//! Sponsored messages replay tests.
//! Split from `tests/replay.rs` — pure code motion.
mod replay_common;
use replay_common::*;

#[test]
fn replay_sponsored_messages_fetch_and_labels() {
    let sink = Arc::new(MemorySink::new());
    let seq = AtomicU64::new(0);
    let session = sponsored_test_session(&sink, &seq);

    let entry = session.sponsored.get(&13).unwrap();
    assert_eq!(entry.messages_between, 3);
    assert_eq!(entry.messages.len(), 2);
    assert_eq!(entry.messages[0].kind_label(), "Sponsored");
    assert_eq!(entry.messages[1].kind_label(), "Recommended");
    assert!(entry.messages[0].can_be_reported);
    assert!(!entry.messages[1].can_be_reported);
    assert_eq!(entry.messages[0].title, "Summer sale");
    assert_eq!(entry.messages[0].sponsor.url, "https://example.com/promo");
    assert_eq!(entry.messages[0].sponsor.info, "Example Ads");
    assert_eq!(entry.messages[0].button_text, "Shop now");
    assert_eq!(entry.messages[0].additional_info, "Ad by Example");

    // Sponsored thumbs join the priority-1 download pass for the open chat.
    let thumbs = session.thumb_file_ids_to_download();
    assert!(thumbs.iter().any(|id| id.0 == 61));

    // Rows preserve the TDLib response vector order (no sort applied).
    let rows = session.open_sponsored_rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].message_id, 9001);
    assert_eq!(rows[1].message_id, 9002);
}

#[test]
fn replay_sponsored_report_option_required_then_ok() {
    let sink = Arc::new(MemorySink::new());
    let seq = AtomicU64::new(0);
    let mut session = sponsored_test_session(&sink, &seq);
    let chat_id = quill::ids::ChatId(13);

    // The Recommended row is not reportable.
    assert!(session.begin_sponsored_report(chat_id, 9002).is_none());
    assert!(session.sponsored_report.is_none());

    assert!(session.begin_sponsored_report(chat_id, 9001).is_some());
    let extra = session.request(RequestPurpose::ReportChatSponsoredMessage, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"reportSponsoredResultOptionRequired","@extra":"{}","title":"Why report?","options":[{{"@type":"reportOption","id":"bWlzLWxlYWQ=","text":"Misleading"}},{{"@type":"reportOption","id":"c3BhbQ==","text":"Spam"}}]}}"#,
            extra.0
        )],
    );
    let flight = session.sponsored_report.clone().unwrap();
    assert_eq!(flight.chat_id, chat_id);
    assert_eq!(flight.message_id, 9001);
    assert_eq!(flight.title, "Why report?");
    assert_eq!(flight.options.len(), 2);
    assert_eq!(flight.options[0].text, "Misleading");
    assert_eq!(flight.options[1].text, "Spam");

    // Follow-up with the chosen option id.
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
    assert!(session.sponsored_report.is_none());
    let outcome = session.last_sponsored_report.clone().unwrap();
    assert_eq!(outcome.chat_id, chat_id);
    assert_eq!(outcome.message_id, 9001);
    assert_eq!(outcome.user_message(), "Report sent");
}

#[test]
fn replay_sponsored_report_failed() {
    let sink = Arc::new(MemorySink::new());
    let seq = AtomicU64::new(0);
    let mut session = sponsored_test_session(&sink, &seq);
    let chat_id = quill::ids::ChatId(13);

    assert!(session.begin_sponsored_report(chat_id, 9001).is_some());
    let extra = session.request(RequestPurpose::ReportChatSponsoredMessage, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"reportSponsoredResultFailed","@extra":"{}"}}"#,
            extra.0
        )],
    );
    assert!(session.sponsored_report.is_none());
    let outcome = session.last_sponsored_report.clone().unwrap();
    assert_eq!(outcome.user_message(), "Could not report this message");
}

#[test]
fn replay_sponsored_report_ads_hidden_and_premium_required() {
    let sink = Arc::new(MemorySink::new());
    let seq = AtomicU64::new(0);
    let mut session = sponsored_test_session(&sink, &seq);
    let chat_id = quill::ids::ChatId(13);

    for (ctor, message) in [
        (
            "reportSponsoredResultAdsHidden",
            "Sponsored messages hidden",
        ),
        (
            "reportSponsoredResultPremiumRequired",
            "Hiding sponsored messages needs Telegram Premium",
        ),
    ] {
        assert!(session.begin_sponsored_report(chat_id, 9001).is_some());
        let extra = session.request(RequestPurpose::ReportChatSponsoredMessage, Some(chat_id));
        apply_all_seq(
            &mut session,
            &sink,
            &seq,
            &[&format!(r#"{{"@type":"{}","@extra":"{}"}}"#, ctor, extra.0)],
        );
        let outcome = session.last_sponsored_report.clone().unwrap();
        assert_eq!(outcome.user_message(), message);
    }
}
