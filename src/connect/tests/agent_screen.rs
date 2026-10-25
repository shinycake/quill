//! While the agent UI driver injects input, a read refused for money is
//! answered like a TDLib error for its `@extra` (`AgentScreen`), so what
//! the caller set loading when it sent the read clears on the next poll:
//! a profile or a channel page opened by an agent's click settles instead
//! of spinning.
use super::super::*;
use super::*;
use crate::agent::ui_driver::guard;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::earn::{EarnPeer, EarnScreen, Load};
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::state::{RequestPurpose, SharedMediaTab, SharedMediaTabStatus};
use crate::telegram::client::copy_and_parse;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn sent_extras(recorder: &RecordingSender) -> HashSet<u64> {
    recorder
        .snapshot()
        .iter()
        .filter_map(|json| serde_json::from_str::<serde_json::Value>(json).ok())
        .filter_map(|value| value.get("@extra")?.as_str()?.parse().ok())
        .collect()
}

fn sent_types(recorder: &RecordingSender) -> Vec<String> {
    recorder
        .snapshot()
        .iter()
        .filter_map(|json| serde_json::from_str::<serde_json::Value>(json).ok())
        .filter_map(|value| value.get("@type")?.as_str().map(str::to_string))
        .collect()
}

#[test]
fn reads_refused_for_money_settle_like_tdlib_errors() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateNewChat","chat":{"id":6,"title":"alice","type":{"@type":"chatTypePrivate","user_id":78},"unread_count":0}}"#,
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    driver.session.media.shared_media.open_for(ChatId(6));
    let _ = guard::take_refusals();

    // An agent's click opens the profile's gifts, the paid-message refund
    // check and a channel's revenue page: all reads, all named for money.
    guard::set_injecting(true);
    let gifts = driver.fetch_profile_tab(SharedMediaTab::Gifts, false);
    let revenue = driver.fetch_paid_message_revenue(78);
    let earn = driver.open_earn_page(EarnPeer::Chat(-1001), EarnScreen::Monetization, false);
    let playlist = driver.fetch_playlist(78, false);
    guard::set_injecting(false);
    assert_eq!(
        (gifts, revenue, earn, playlist),
        (Ok(()), Ok(()), Ok(()), Ok(()))
    );
    let sent = sent_types(&recorder);
    for refused in [
        "getReceivedGifts",
        "getPaidMessageRevenue",
        "getStarRevenueStatistics",
    ] {
        assert!(
            !sent.iter().any(|t| t == refused),
            "{refused} went out: {sent:?}"
        );
    }
    assert!(sent.iter().any(|t| t == "getUserProfileAudios"), "{sent:?}");
    assert!(!guard::take_refusals().is_empty());
    let gifts_status = |driver: &ConnectDriver<Arc<RecordingSender>>| {
        driver
            .session
            .media
            .shared_media
            .tabs
            .get(SharedMediaTab::Gifts.index())
            .map(|tab| tab.status)
    };
    let stars = |driver: &ConnectDriver<Arc<RecordingSender>>| {
        driver
            .session
            .payments
            .earn
            .page
            .as_ref()
            .map(|page| page.stars.clone())
    };
    assert_eq!(gifts_status(&driver), Some(SharedMediaTabStatus::Loading));
    assert!(matches!(stars(&driver), Some(Load::Loading)));

    // The next poll ingests the answers: nothing is left loading or pending.
    assert!(driver.ingest_refused_answers());
    assert!(!driver.ingest_refused_answers(), "answered once");
    assert_ne!(gifts_status(&driver), Some(SharedMediaTabStatus::Loading));
    assert!(
        matches!(stars(&driver), Some(Load::Failed(_))),
        "{:?}",
        stars(&driver)
    );
    assert!(
        !driver
            .session
            .requests
            .has_purpose_for_user(RequestPurpose::GetPaidMessageRevenue, 78)
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// An agent's click opens a channel it owns and a contact's profile, each
/// with its shared-media gallery: whatever was refused for money is
/// answered, so no request is left pending that never went out.
#[test]
fn a_channel_and_a_profile_opened_by_the_agent_leave_nothing_pending() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    for json in [
        r#"{"@type":"updateNewChat","chat":{"id":6,"title":"alice","type":{"@type":"chatTypePrivate","user_id":78},"unread_count":0}}"#,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":10,"is_channel":true,"status":{"@type":"chatMemberStatusCreator","is_anonymous":false,"is_member":true}}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":-10010,"title":"news","type":{"@type":"chatTypeSupergroup","supergroup_id":10,"is_channel":true},"unread_count":0}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &sink).unwrap())
            .unwrap();
    }
    let _ = guard::take_refusals();

    for chat in [ChatId(-10010), ChatId(6)] {
        guard::set_injecting(true);
        let opened = driver.select_chat(chat).map(|_| ());
        let gallery = driver.open_shared_media().map(|_| ());
        let gifts = driver.fetch_profile_tab(SharedMediaTab::Gifts, false);
        guard::set_injecting(false);
        assert_eq!(
            (opened, gallery, gifts),
            (Ok(()), Ok(()), Ok(())),
            "{chat:?}"
        );
        driver.ingest_refused_answers();
        let sent = sent_extras(&recorder);
        let stuck: Vec<_> = (0..10_000)
            .map(crate::ids::RequestId)
            .filter(|id| !sent.contains(&id.0))
            .filter_map(|id| driver.session.requests.purpose(id))
            .collect();
        assert!(
            stuck.is_empty(),
            "{chat:?}: refused and still pending: {stuck:?}"
        );
        assert_ne!(
            driver
                .session
                .media
                .shared_media
                .tabs
                .get(SharedMediaTab::Gifts.index())
                .map(|tab| tab.status),
            Some(SharedMediaTabStatus::Loading),
            "{chat:?}"
        );
    }
    assert!(!guard::take_refusals().is_empty(), "something was refused");
    std::fs::remove_dir_all(dir).unwrap();
}
