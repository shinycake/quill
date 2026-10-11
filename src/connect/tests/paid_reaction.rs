//! Connect-driver tests: Star reactions with Undo and the pack list
//! (recorded JSON, nothing live).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, MessageId};
use crate::paid_reaction::{ACCUMULATE_MS, PaidSender};
use crate::platform::MemorySecretStore;
use crate::premium_hub::StarAmount;
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Recorder = Arc<RecordingSender>;

fn driver_parts() -> (
    ConnectDriver<Recorder>,
    Recorder,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
) {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let dyn_sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    (driver, recorder, dyn_sink, seq)
}

fn feed(
    driver: &mut ConnectDriver<Recorder>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    json: &str,
) {
    driver
        .ingest(copy_and_parse(json, seq, sink).unwrap())
        .unwrap();
}

fn sent_count(recorder: &RecordingSender, type_name: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| {
            serde_json::from_str::<serde_json::Value>(json).is_ok_and(|v| v["@type"] == type_name)
        })
        .count()
}

#[test]
fn a_tap_is_held_then_committed_after_the_window() {
    let (mut driver, recorder, _sink, _seq) = driver_parts();
    driver
        .add_paid_reaction(ChatId(-100), MessageId(7), 1, None, 1_000)
        .unwrap();
    let add = sent_request(&recorder, "addPendingPaidMessageReaction");
    assert_eq!(add["star_count"], 1);
    assert!(add["type"].is_null());
    assert_eq!(
        sent_count(&recorder, "commitPendingPaidMessageReactions"),
        0
    );
    assert!(!driver.paid_reaction_tick(1_000 + ACCUMULATE_MS - 1));
    assert_eq!(
        sent_count(&recorder, "commitPendingPaidMessageReactions"),
        0
    );
    assert!(driver.paid_reaction_tick(1_000 + ACCUMULATE_MS));
    let commit = sent_request(&recorder, "commitPendingPaidMessageReactions");
    assert_eq!(commit["message_id"], 7);
    assert!(!driver.session.payments.paid.has_pending());
    // Only once.
    assert!(!driver.paid_reaction_tick(u64::MAX));
}

#[test]
fn undo_removes_the_pending_stars_and_nothing_commits() {
    let (mut driver, recorder, _sink, _seq) = driver_parts();
    driver
        .add_paid_reaction(
            ChatId(-100),
            MessageId(7),
            3,
            Some(PaidSender::Anonymous),
            0,
        )
        .unwrap();
    driver
        .undo_paid_reaction(ChatId(-100), MessageId(7))
        .unwrap();
    let undo = sent_request(&recorder, "removePendingPaidMessageReactions");
    assert_eq!(undo["chat_id"], -100);
    assert!(!driver.paid_reaction_tick(u64::MAX));
    assert_eq!(
        sent_count(&recorder, "commitPendingPaidMessageReactions"),
        0
    );
    // A second Undo sends nothing more.
    driver
        .undo_paid_reaction(ChatId(-100), MessageId(7))
        .unwrap();
    assert_eq!(
        sent_count(&recorder, "removePendingPaidMessageReactions"),
        1
    );
}

#[test]
fn a_balance_too_small_refuses_before_any_request() {
    let (mut driver, recorder, _sink, _seq) = driver_parts();
    driver.session.payments.hub.balance = Some(StarAmount { stars: 4, nanos: 0 });
    assert!(
        driver
            .add_paid_reaction(ChatId(-100), MessageId(7), 5, None, 0)
            .is_err()
    );
    assert_eq!(sent_count(&recorder, "addPendingPaidMessageReaction"), 0);
    assert!(driver.session.payments.paid.note.is_some());
    assert!(
        driver
            .add_paid_reaction(ChatId(-100), MessageId(7), 4, None, 0)
            .is_ok()
    );
    assert!(driver.session.payments.paid.note.is_none());
    // The 4 waiting Stars are reserved.
    assert!(
        driver
            .add_paid_reaction(ChatId(-100), MessageId(8), 1, None, 0)
            .is_err()
    );
}

#[test]
fn a_refused_tap_is_rolled_back() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver
        .add_paid_reaction(ChatId(-100), MessageId(7), 1, None, 0)
        .unwrap();
    let add = sent_request(&recorder, "addPendingPaidMessageReaction");
    feed(
        &mut driver,
        &seq,
        &sink,
        &serde_json::json!({
            "@type": "error", "@extra": add["@extra"], "code": 400, "message": "REACTION_INVALID"
        })
        .to_string(),
    );
    assert!(!driver.session.payments.paid.has_pending());
    assert!(
        driver
            .session
            .payments
            .paid
            .note
            .as_deref()
            .is_some_and(|note| note.contains("Couldn't send Stars"))
    );
}

#[test]
fn star_packs_load_once_and_land_in_the_state() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    assert!(driver.maybe_fetch_star_packs().unwrap().is_some());
    assert!(driver.maybe_fetch_star_packs().unwrap().is_none());
    let request = sent_request(&recorder, "getStarPaymentOptions");
    feed(
        &mut driver,
        &seq,
        &sink,
        &serde_json::json!({
            "@type": "starPaymentOptions", "@extra": request["@extra"],
            "options": [{"@type": "starPaymentOption", "currency": "USD", "amount": 199,
                         "star_count": 100, "store_product_id": "", "is_additional": false}]
        })
        .to_string(),
    );
    let packs = &driver.session.payments.star_packs;
    assert!(packs.loaded && !packs.loading);
    assert_eq!(packs.options[0].star_count, 100);
    assert!(driver.maybe_fetch_star_packs().unwrap().is_none());
}

#[test]
fn owned_star_count_updates_the_balance() {
    let (mut driver, _recorder, sink, seq) = driver_parts();
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateOwnedStarCount","star_amount":{"@type":"starAmount","star_count":321,"nanostar_count":0}}"#,
    );
    assert_eq!(
        driver.session.payments.hub.balance,
        Some(StarAmount {
            stars: 321,
            nanos: 0
        })
    );
}
