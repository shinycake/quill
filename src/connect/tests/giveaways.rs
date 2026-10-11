//! Giveaways: the info box asks TDLib, the price options only inform, and a
//! prepaid giveaway starts only on the explicit call. No test path pays.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::giveaway::GiveawayDraft;
use crate::platform::MemorySecretStore;
use crate::state::GiveawayLaunch;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::{
    GiveawayInfoData, GiveawayPrize, GiveawaySummary, ParsedPrepaidGiveaway,
};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const NOW: i32 = 1_700_000_000;

fn feed(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    value: serde_json::Value,
) {
    driver
        .ingest(copy_and_parse(&value.to_string(), seq, sink).unwrap())
        .unwrap();
}

fn count_sent(recorder: &RecordingSender, ty: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter_map(|j| serde_json::from_str::<serde_json::Value>(j).ok())
        .filter(|v| v["@type"] == ty)
        .count()
}

fn summary() -> GiveawaySummary {
    GiveawaySummary {
        boosted_chat_id: -100,
        info_message_id: 0,
        additional_chat_count: 0,
        selection_date: NOW + 1000,
        only_new_members: false,
        has_public_winners: true,
        prize_description: String::new(),
        winner_count: 3,
        unclaimed_count: 0,
        prize: GiveawayPrize::Premium { month_count: 3 },
        finished: false,
        was_refunded: false,
    }
}

fn prepaid_draft() -> GiveawayDraft {
    GiveawayDraft::new(
        -100,
        Some(ParsedPrepaidGiveaway {
            id: 4_611_686_018_427_387_904,
            winner_count: 3,
            prize: GiveawayPrize::Premium { month_count: 3 },
            boost_count: 12,
            payment_date: 1,
        }),
        NOW,
    )
}

#[test]
fn the_info_box_asks_tdlib_and_keeps_the_answer() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);

    assert!(driver.request_giveaway_info(-100, 0, summary()).is_err());
    let extra = driver.request_giveaway_info(-100, 55, summary()).unwrap();
    let sent = sent_request(&recorder, "getGiveawayInfo");
    assert_eq!(
        (sent["chat_id"].as_i64(), sent["message_id"].as_i64()),
        (Some(-100), Some(55))
    );
    assert!(
        driver
            .session
            .payments
            .giveaway
            .info
            .as_ref()
            .unwrap()
            .loading
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"giveawayInfoOngoing","@extra":extra.as_extra(),"creation_date":5,
            "status":{"@type":"giveawayParticipantStatusParticipating"},"is_ended":false}),
    );
    let lookup = driver.session.payments.giveaway.info.as_ref().unwrap();
    assert!(!lookup.loading);
    assert!(matches!(
        lookup.info,
        Some(GiveawayInfoData::Ongoing { .. })
    ));

    let extra = driver.request_giveaway_info(-100, 56, summary()).unwrap();
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"error","@extra":extra.as_extra(),"code":400,"message":"MESSAGE_ID_INVALID"}),
    );
    let lookup = driver.session.payments.giveaway.info.as_ref().unwrap();
    assert!(!lookup.loading && lookup.info.is_none() && lookup.error.is_some());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn price_options_only_inform() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);

    driver.request_giveaway_options(-100).unwrap();
    assert!(driver.session.payments.giveaway.options.loading());
    let premium = sent_request(&recorder, "getPremiumGiveawayPaymentOptions");
    assert_eq!(premium["boosted_chat_id"], -100);
    let premium_extra = premium["@extra"].clone();
    let stars_extra = sent_request(&recorder, "getStarGiveawayPaymentOptions")["@extra"].clone();
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"premiumGiveawayPaymentOptions","@extra":premium_extra,
            "options":[{"currency":"USD","amount":1500,"winner_count":5,"month_count":3}]}),
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"error","@extra":stars_extra,"code":400,"message":"BOOST_NOT_MODIFIED"}),
    );
    let options = &driver.session.payments.giveaway.options;
    assert!(!options.loading());
    assert_eq!(options.premium.len(), 1);
    assert!(options.stars.is_empty() && options.stars_error.is_some());
    for ty in ["getPaymentForm", "sendPaymentForm", "launchPrepaidGiveaway"] {
        assert_eq!(count_sent(&recorder, ty), 0);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_prepaid_giveaway_launches_only_on_the_explicit_call() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.session.groups.chat_boost_status.insert(-100, (1, 2));

    // A new giveaway (not prepaid) never launches.
    let paid = GiveawayDraft::new(-100, None, NOW);
    assert!(driver.launch_prepaid_giveaway(&paid, NOW).is_err());
    // A date in the past is refused before anything is sent.
    let mut late = prepaid_draft();
    late.selection_date = NOW - 5;
    assert!(driver.launch_prepaid_giveaway(&late, NOW).is_err());
    assert_eq!(count_sent(&recorder, "launchPrepaidGiveaway"), 0);

    let mut draft = prepaid_draft();
    draft.prize_description = "A mug".into();
    let extra = driver.launch_prepaid_giveaway(&draft, NOW).unwrap();
    let sent = sent_request(&recorder, "launchPrepaidGiveaway");
    assert_eq!(sent["giveaway_id"], "4611686018427387904");
    assert_eq!(sent["winner_count"], 3);
    assert_eq!(sent["star_count"], 0);
    assert_eq!(sent["parameters"]["prize_description"], "A mug");
    assert_eq!(sent["parameters"]["boosted_chat_id"], -100);
    assert!(matches!(
        driver.session.payments.giveaway.launch,
        GiveawayLaunch::Launching { chat_id: -100 }
    ));
    // A second click while it runs sends nothing.
    assert!(driver.launch_prepaid_giveaway(&draft, NOW).is_err());
    assert_eq!(count_sent(&recorder, "launchPrepaidGiveaway"), 1);

    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"ok","@extra":extra.as_extra()}),
    );
    assert_eq!(
        driver.session.payments.giveaway.launch,
        GiveawayLaunch::Launched
    );
    // The cached boost numbers are dropped so the page asks again.
    assert!(!driver.session.groups.chat_boost_status.contains_key(&-100));
    for ty in ["getPaymentForm", "sendPaymentForm"] {
        assert_eq!(count_sent(&recorder, ty), 0);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_failed_launch_reports_the_reason() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let extra = driver
        .launch_prepaid_giveaway(&prepaid_draft(), NOW)
        .unwrap();
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"error","@extra":extra.as_extra(),"code":400,"message":"GIVEAWAY_ID_INVALID"}),
    );
    assert!(matches!(
        driver.session.payments.giveaway.launch,
        GiveawayLaunch::Failed(ref text) if text.starts_with("Couldn't start")
    ));
    // After a failure the user can try again.
    assert!(
        driver
            .launch_prepaid_giveaway(&prepaid_draft(), NOW)
            .is_ok()
    );
    std::fs::remove_dir_all(dir).unwrap();
}
