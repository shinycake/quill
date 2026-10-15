//! The value of an upgraded gift: the request shape, the answer opening
//! the box, stale answers and failures (recorded JSON only).
use super::super::*;
use super::support::{prepared_tmp, ready_driver, sent_request};
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::premium_hub::gift_value::GiftValueLookup;
use crate::telegram::client::copy_and_parse;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const LAST: &str = include_str!("../../../tests/fixtures/gift_value/value_info_last.json");
const AVERAGE: &str = include_str!("../../../tests/fixtures/gift_value/value_info_average.json");

fn answer(extra: &Value, fixture: &str) -> String {
    let mut body: Value = serde_json::from_str(fixture).unwrap();
    body["@extra"] = extra.clone();
    body.to_string()
}

fn sent_count(recorder: &RecordingSender) -> usize {
    recorder
        .snapshot()
        .into_iter()
        .filter(|json| json.contains("getUpgradedGiftValueInfo"))
        .count()
}

fn lookup(name: &str) -> GiftValueLookup {
    GiftValueLookup {
        name: name.into(),
        title: "Plush Pepe".into(),
        full_name: format!("Plush Pepe #{name}"),
        regular_gift_id: 7001,
        ..GiftValueLookup::default()
    }
}

#[test]
fn the_answer_opens_the_box_and_stale_answers_are_dropped() {
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

    // The read carries the gift's unique name, nothing else.
    assert!(driver.request_gift_value(lookup("A-1")).unwrap().is_some());
    let first = sent_request(&recorder, "getUpgradedGiftValueInfo");
    assert_eq!(first["name"], "A-1");
    let keys: Vec<&String> = first.as_object().unwrap().keys().collect();
    assert_eq!(keys.len(), 3, "{keys:?}");
    let value = driver.session.payments.hub.gift_value.as_ref().unwrap();
    assert!(value.loading && !value.open);

    // A second click while it runs sends nothing.
    assert!(driver.request_gift_value(lookup("A-1")).unwrap().is_none());
    assert_eq!(sent_count(&recorder), 1);

    // Another gift replaces the lookup; the first answer is stale.
    assert!(driver.request_gift_value(lookup("B-2")).unwrap().is_some());
    let second = sent_request(&recorder, "getUpgradedGiftValueInfo");
    assert_eq!(second["name"], "B-2");
    ingest(&mut driver, answer(&first["@extra"], LAST));
    let value = driver.session.payments.hub.gift_value.as_ref().unwrap();
    assert_eq!(value.name, "B-2");
    assert!(value.loading && value.info.is_none() && !value.open);

    ingest(&mut driver, answer(&second["@extra"], AVERAGE));
    let value = driver.session.payments.hub.gift_value.as_ref().unwrap();
    assert!(!value.loading && value.open);
    assert_eq!(value.info.as_ref().unwrap().average_sale_price, 214_900);

    // Closed and asked again: reopens from what is known, no request.
    driver
        .session
        .payments
        .hub
        .gift_value
        .as_mut()
        .unwrap()
        .open = false;
    assert!(driver.request_gift_value(lookup("B-2")).unwrap().is_none());
    assert_eq!(sent_count(&recorder), 2);
    assert!(
        driver
            .session
            .payments
            .hub
            .gift_value
            .as_ref()
            .unwrap()
            .open
    );
}

#[test]
fn a_failure_shows_under_the_row_and_a_stale_one_is_ignored() {
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
    let error = |extra: &Value| {
        json!({"@type": "error", "code": 400, "message": "STARGIFT_SLUG_INVALID", "@extra": extra})
            .to_string()
    };

    driver.request_gift_value(lookup("A-1")).unwrap();
    let first = sent_request(&recorder, "getUpgradedGiftValueInfo");
    driver.request_gift_value(lookup("B-2")).unwrap();
    let second = sent_request(&recorder, "getUpgradedGiftValueInfo");

    ingest(&mut driver, error(&first["@extra"]));
    let value = driver.session.payments.hub.gift_value.as_ref().unwrap();
    assert!(value.loading && value.error.is_none());

    ingest(&mut driver, error(&second["@extra"]));
    let value = driver.session.payments.hub.gift_value.as_ref().unwrap();
    assert!(!value.loading && !value.open);
    let shown = value.error.as_deref().unwrap();
    assert!(shown.starts_with("Couldn't load the value: "), "{shown}");

    // A retry after the failure sends again.
    assert!(driver.request_gift_value(lookup("B-2")).unwrap().is_some());
    assert_eq!(sent_count(&recorder), 3);
}
