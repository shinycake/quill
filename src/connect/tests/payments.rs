//! Slice payments: "clear saved payment/shipping info" dispatch.
use super::super::*;
use super::support::{prepared_tmp, ready_driver, sent_request, test_credentials};
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::AccountKey;
use crate::platform::MemorySecretStore;
use crate::state::Session;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn clear_saved_payment_info_dispatches_both_deletes() {
    // The clear action must fire BOTH parameterless `= Ok` constructors
    // (`deleteSavedOrderInfo` + `deleteSavedCredentials`) with their own
    // `@extra` ids so the answers correlate.
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);

    assert_eq!(driver.clear_saved_payment_info().unwrap(), 2);

    let order = sent_request(&recorder, "deleteSavedOrderInfo");
    let creds = sent_request(&recorder, "deleteSavedCredentials");
    assert_ne!(order["@extra"], creds["@extra"]);
}

#[test]
fn clear_saved_payment_info_refuses_without_ready_path() {
    // The driver gate refuses before anything is registered or sent.
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let session = Session::new(AccountKey::primary(), sink);
    let mut driver = ConnectDriver::new(
        session,
        Arc::new(RecordingSender::new()),
        test_credentials(),
        prepared,
    );

    assert_eq!(
        driver.clear_saved_payment_info(),
        Err(ConnectSendError::InvalidRequest)
    );
    assert!(driver.session.requests.pending.is_empty());
}

/// Slice parity:gifts-signed-comment: `sendResoldGift` sends the personal
/// `text` comment and the right `GiftResalePrice` variant.
#[test]
fn send_resold_gift_dispatches_with_comment_and_star_price() {
    use crate::ids::UserId;
    use crate::telegram::requests::GiftResalePrice;
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);

    let extra = driver
        .request_send_resold_gift(
            "Durov's Cap",
            UserId(42),
            GiftResalePrice::Star(500),
            "Happy birthday!",
            false,
        )
        .expect("ready driver sends");
    let req = sent_request(&recorder, "sendResoldGift");
    assert_eq!(req["@extra"], extra.0.to_string());
    assert_eq!(req["gift_name"], "Durov's Cap");
    assert_eq!(req["owner_id"]["@type"], "messageSenderUser");
    assert_eq!(req["owner_id"]["user_id"], 42);
    assert_eq!(req["price"]["@type"], "giftResalePriceStar");
    assert_eq!(req["price"]["star_count"], 500);
    assert_eq!(req["text"]["text"], "Happy birthday!");
    assert_eq!(req["is_private"], false);
}

/// Slice parity:gifts-signed-comment: TON price maps to `giftResalePriceGram`.
#[test]
fn send_resold_gift_dispatches_ton_price() {
    use crate::ids::UserId;
    use crate::telegram::requests::GiftResalePrice;
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);

    driver
        .request_send_resold_gift(
            "Plush Pepe",
            UserId(7),
            GiftResalePrice::Gram(100),
            "",
            true,
        )
        .expect("ready driver sends");
    let req = sent_request(&recorder, "sendResoldGift");
    assert_eq!(req["price"]["@type"], "giftResalePriceGram");
    assert_eq!(req["price"]["gram_cent_count"], 100);
    assert_eq!(req["text"]["text"], "");
    assert_eq!(req["is_private"], true);
}
