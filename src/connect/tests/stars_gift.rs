//! The Gift Stars dialog: the contact, the packs for them and the refusals
//! (recorded JSON, nothing live and no real payment).
use super::super::*;
use super::support::{prepared_tmp, ready_driver, sent_request};
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn a_contact_is_asked_for_their_packs_and_the_answer_lands() {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.open_stars_gift().unwrap();
    assert!(driver.session.payments.stars_gift.open);
    sent_request(&recorder, "getContacts");
    driver.choose_stars_gift_user(42, "Ada").unwrap();
    let request = sent_request(&recorder, "getStarGiftPaymentOptions");
    assert_eq!(request["user_id"], 42);
    assert!(driver.session.payments.stars_gift.loading);
    let answer = json!({
        "@type": "starPaymentOptions", "@extra": request["@extra"],
        "options": [{"@type": "starPaymentOption", "currency": "USD", "amount": 199,
                     "star_count": 100, "store_product_id": "", "is_additional": false}]
    });
    driver
        .ingest(copy_and_parse(&answer.to_string(), &seq, &sink).unwrap())
        .unwrap();
    let gift = &driver.session.payments.stars_gift;
    assert!(!gift.loading);
    assert_eq!(gift.target.as_ref().map(|t| t.user_id), Some(42));
    assert_eq!(gift.options[0].star_count, 100);
    // The packs for a gift never land in the "Buy more Stars" list.
    assert!(driver.session.payments.star_packs.options.is_empty());
    driver.close_stars_gift();
    assert!(!driver.session.payments.stars_gift.open);
}

#[test]
fn yourself_and_a_closed_dialog_are_refused() {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    assert_eq!(
        driver.choose_stars_gift_user(42, "Ada"),
        Err(ConnectSendError::InvalidRequest)
    );
    driver.open_stars_gift().unwrap();
    driver.session.my_user_id = Some(7);
    assert_eq!(
        driver.choose_stars_gift_user(7, "Me"),
        Err(ConnectSendError::InvalidRequest)
    );
    assert!(
        recorder
            .snapshot()
            .iter()
            .all(|j| !j.contains("getStarGiftPaymentOptions"))
    );
}
