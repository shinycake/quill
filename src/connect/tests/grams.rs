//! Settings → My Grams: the balance probe, the history pages and the
//! revenue request (recorded JSON only; nothing moves Grams).
use super::super::*;
use super::support::{prepared_tmp, ready_driver, sent_request};
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::premium_hub::TxFilter;
use crate::telegram::client::copy_and_parse;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const FIXTURE: &str = include_str!("../../../tests/fixtures/grams/ton_transactions.json");

fn answer(extra: &Value, mut body: Value) -> String {
    body["@extra"] = extra.clone();
    body.to_string()
}

fn sent_of(recorder: &RecordingSender, type_name: &str) -> Vec<Value> {
    recorder
        .snapshot()
        .into_iter()
        .map(|json| serde_json::from_str::<Value>(&json).unwrap())
        .filter(|value| value["@type"] == type_name)
        .collect()
}

#[test]
fn the_probe_fills_the_balance_once_and_the_screen_pages() {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: String| {
        driver
            .ingest(copy_and_parse(&json, &seq, &sink).unwrap())
            .unwrap();
    };

    // The poll tick probes only while Settings is open, and once.
    assert!(!driver.grams_tick(false));
    assert!(sent_of(&recorder, "getTonTransactions").is_empty());
    assert!(driver.grams_tick(true));
    assert!(!driver.grams_tick(true));
    assert!(driver.probe_grams_balance().unwrap().is_none());
    assert_eq!(sent_of(&recorder, "getTonTransactions").len(), 1);
    let probe = sent_request(&recorder, "getTonTransactions");
    assert_eq!(probe["limit"], 1);
    ingest(
        &mut driver,
        answer(
            &probe["@extra"],
            json!({"@type": "tonTransactions", "gram_amount": 750000000, "transactions": [], "next_offset": "x"}),
        ),
    );
    let grams = &driver.session.payments.hub.grams;
    assert_eq!(grams.balance_nano, Some(750_000_000));
    // The probe never fills the list.
    assert!(grams.entries.is_empty() && !grams.loaded);

    driver.open_grams(false).unwrap();
    let first = sent_request(&recorder, "getTonTransactions");
    assert_eq!(first["limit"], 50);
    assert!(first.get("direction").is_none());
    let revenue = sent_request(&recorder, "getGramRevenueStatistics");
    assert_eq!(revenue["is_dark"], false);
    ingest(
        &mut driver,
        answer(
            &first["@extra"],
            serde_json::from_str::<Value>(FIXTURE).unwrap(),
        ),
    );
    ingest(
        &mut driver,
        answer(
            &revenue["@extra"],
            json!({"@type": "gramRevenueStatistics",
                   "revenue_by_day_graph": {"@type": "statisticalGraphError", "error_message": ""},
                   "status": {"@type": "gramRevenueStatus", "total_amount": "1", "balance_amount": "1",
                              "available_amount": "5000000000", "withdrawal_enabled": true},
                   "usd_rate": 0.00000035}),
        ),
    );
    let grams = &driver.session.payments.hub.grams;
    assert!(grams.open && grams.loaded && !grams.loading);
    assert_eq!(grams.entries.len(), 11);
    assert_eq!(grams.balance_nano, Some(12_750_000_000));
    assert_eq!(
        grams.withdrawable().map(|r| r.available_nano),
        Some(5_000_000_000)
    );

    // Names the page uses but the session lacks are asked for once each.
    assert!(driver.grams_tick(false));
    assert!(!driver.grams_tick(false));
    let mut users: Vec<i64> = sent_of(&recorder, "getUser")
        .iter()
        .filter_map(|v| v["user_id"].as_i64())
        .collect();
    users.sort_unstable();
    assert_eq!(users, [11, 12]);
    let chats = sent_of(&recorder, "getChat");
    assert_eq!(chats.len(), 1);
    assert_eq!(chats[0]["chat_id"], 13);
    let get_user = sent_of(&recorder, "getUser")
        .into_iter()
        .find(|v| v["user_id"] == 12)
        .unwrap();
    ingest(
        &mut driver,
        answer(
            &get_user["@extra"],
            json!({"@type": "user", "id": 12, "first_name": "Leo", "last_name": "Park",
                   "type": {"@type": "userTypeRegular"}}),
        ),
    );
    assert_eq!(
        driver.session.user(12).map(|u| u.display_name()),
        Some("Leo Park".to_string())
    );
    ingest(
        &mut driver,
        answer(
            &chats[0]["@extra"],
            json!({"@type": "error", "code": 400, "message": "CHAT_NOT_FOUND"}),
        ),
    );
    assert!(
        driver
            .session
            .payments
            .hub
            .grams
            .missing_chats
            .contains(&13)
    );

    // The next page carries the offset and appends.
    driver.fetch_more_grams().unwrap();
    let more = sent_request(&recorder, "getTonTransactions");
    assert_eq!(more["offset"], "page-2");
    // A tab switch in flight makes that answer stale.
    driver.set_grams_filter(TxFilter::Incoming).unwrap();
    let incoming = sent_request(&recorder, "getTonTransactions");
    assert_eq!(
        incoming["direction"]["@type"],
        "transactionDirectionIncoming"
    );
    ingest(
        &mut driver,
        answer(
            &more["@extra"],
            json!({"@type": "tonTransactions", "gram_amount": 1, "transactions": [], "next_offset": ""}),
        ),
    );
    assert!(driver.session.payments.hub.grams.entries.is_empty());
    assert!(driver.session.payments.hub.grams.loading);
    // Another switch: an error for the older tab leaves the live one
    // loading.
    driver.set_grams_filter(TxFilter::Outgoing).unwrap();
    let outgoing = sent_request(&recorder, "getTonTransactions");
    ingest(
        &mut driver,
        answer(
            &incoming["@extra"],
            json!({"@type": "error", "code": 400, "message": "BAD_REQUEST"}),
        ),
    );
    let grams = &driver.session.payments.hub.grams;
    assert!(grams.loading && grams.error.is_none());
    ingest(
        &mut driver,
        answer(
            &outgoing["@extra"],
            json!({"@type": "error", "code": 400, "message": "BAD_REQUEST"}),
        ),
    );
    let grams = &driver.session.payments.hub.grams;
    assert!(!grams.loading);
    assert!(grams.error.is_some());

    // Only reads were sent.
    for json in recorder.snapshot() {
        let value: Value = serde_json::from_str(&json).unwrap();
        let ty = value["@type"].as_str().unwrap_or_default();
        assert!(
            !matches!(ty, "getGramWithdrawalUrl" | "transferGift" | "sendGift"),
            "{ty}"
        );
    }
}
