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

#[test]
fn marketplace_comment_purchase_binds_price_recipient_and_correlated_result() {
    use crate::ids::ChatId;
    use crate::marketplace::GiftPrice;
    use crate::telegram::client::copy_and_parse;
    use serde_json::{Value, json};
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let apply = |driver: &mut ConnectDriver<Arc<RecordingSender>>, value: Value| {
        driver
            .ingest(copy_and_parse(&value.to_string(), &seq, &sink).unwrap())
            .unwrap();
    };
    apply(
        &mut driver,
        json!({"@type":"updateNewChat","chat":{"id":11,"title":"Recipient","type":{"@type":"chatTypePrivate","user_id":42}}}),
    );
    assert!(
        driver
            .fetch_marketplace_gift(ChatId(11), "bad/name")
            .is_err()
    );
    let quote = driver
        .fetch_marketplace_gift(ChatId(11), "PlushPepe-123")
        .unwrap();
    assert!(
        driver
            .fetch_marketplace_gift(ChatId(11), "PlushPepe-124")
            .is_err()
    );
    let option = sent_request(&recorder, "getOption");
    assert_eq!(option["name"], "gift_text_length_max");
    apply(
        &mut driver,
        json!({"@type":"optionValueInteger","@extra":option["@extra"],"value":"2"}),
    );
    apply(
        &mut driver,
        json!({"@type":"upgradedGift","@extra":quote.as_extra(),"name":"PlushPepe-123","title":"Plush Pepe","resale_parameters":{"@type":"giftResaleParameters","star_count":25,"gram_cent_count":0,"gram_only":false}}),
    );
    assert!(
        driver
            .buy_marketplace_gift(GiftPrice::Stars(24), "Hi", true)
            .is_err()
    );
    assert!(
        driver
            .buy_marketplace_gift(GiftPrice::Stars(25), "Too long", true)
            .is_err()
    );
    let buy = driver
        .buy_marketplace_gift(GiftPrice::Stars(25), "🎁!", true)
        .unwrap();
    let request = sent_request(&recorder, "sendResoldGift");
    assert_eq!(
        request["owner_id"],
        json!({"@type":"messageSenderUser","user_id":42})
    );
    assert_eq!(
        request["text"],
        json!({"@type":"formattedText","text":"🎁!","entities":[]})
    );
    assert_eq!(
        request["price"],
        json!({"@type":"giftResalePriceStar","star_count":25})
    );
    assert_eq!(request["is_private"], true);
    assert!(
        driver
            .buy_marketplace_gift(GiftPrice::Stars(25), "🎁!", true)
            .is_err()
    );
    let sent = recorder.snapshot().len();
    apply(
        &mut driver,
        json!({"@type":"giftResaleResultPriceIncreased","@extra":buy.as_extra(),"price":{"@type":"giftResalePriceStar","star_count":40}}),
    );
    assert_eq!(
        recorder.snapshot().len(),
        sent,
        "a higher price must never auto-submit"
    );
    assert!(!driver.session.marketplace_gift.as_ref().unwrap().completed);
    assert!(
        driver
            .buy_marketplace_gift(GiftPrice::Stars(25), "Hi", false)
            .is_err()
    );
    apply(
        &mut driver,
        json!({"@type":"giftResaleResultOk","received_gift_id":""}),
    );
    assert!(
        !driver.session.marketplace_gift.as_ref().unwrap().completed,
        "unsolicited success is ignored"
    );
    let retry = driver
        .buy_marketplace_gift(GiftPrice::Stars(40), "Hi", false)
        .unwrap();
    apply(
        &mut driver,
        json!({"@type":"error","@extra":retry.as_extra(),"code":400,"message":"GIFT_NOT_AVAILABLE"}),
    );
    assert!(!driver.session.marketplace_gift.as_ref().unwrap().sending);
    assert!(!driver.session.marketplace_gift.as_ref().unwrap().completed);
    let success = driver
        .buy_marketplace_gift(GiftPrice::Stars(40), "Hi", false)
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "sendResoldGift")["is_private"],
        false
    );
    apply(
        &mut driver,
        json!({"@type":"giftResaleResultOk","@extra":success.as_extra(),"received_gift_id":""}),
    );
    assert!(
        driver.session.marketplace_gift.as_ref().unwrap().completed,
        "receipt id is empty for gifts to other users"
    );
    assert!(
        driver
            .buy_marketplace_gift(GiftPrice::Stars(40), "Hi", false)
            .is_err()
    );
    apply(
        &mut driver,
        json!({"@type":"updateNewChat","chat":{"id":22,"title":"Channel","type":{"@type":"chatTypeSupergroup","supergroup_id":22,"is_channel":true}}}),
    );
    let quote = driver
        .fetch_marketplace_gift(ChatId(22), "PlushPepe-124")
        .unwrap();
    apply(
        &mut driver,
        json!({"@type":"upgradedGift","@extra":quote.as_extra(),"name":"PlushPepe-124","resale_parameters":{"star_count":25,"gram_cent_count":200,"gram_only":true}}),
    );
    assert!(
        driver
            .session
            .marketplace_gift
            .as_ref()
            .unwrap()
            .quote
            .as_ref()
            .unwrap()
            .stars
            .is_none()
    );
    assert!(
        driver
            .buy_marketplace_gift(GiftPrice::Stars(25), "Hi", true)
            .is_err()
    );
    let buy = driver
        .buy_marketplace_gift(GiftPrice::TonCents(200), "Hi", true)
        .unwrap();
    let request = sent_request(&recorder, "sendResoldGift");
    assert_eq!(
        request["owner_id"],
        json!({"@type":"messageSenderChat","chat_id":22})
    );
    assert_eq!(
        request["price"],
        json!({"@type":"giftResalePriceGram","gram_cent_count":200})
    );
    assert_eq!(GiftPrice::TonCents(200).label(), "2.00 TON");
    apply(
        &mut driver,
        json!({"@type":"giftResaleResultPriceIncreased","@extra":buy.as_extra(),"price":{"@type":"giftResalePriceGram","gram_cent_count":-1}}),
    );
    assert!(
        driver
            .session
            .marketplace_gift
            .as_ref()
            .unwrap()
            .quote
            .is_none()
    );
    assert!(
        driver
            .buy_marketplace_gift(GiftPrice::TonCents(200), "Hi", true)
            .is_err()
    );
    let bad_quote = driver
        .fetch_marketplace_gift(ChatId(22), "PlushPepe-125")
        .unwrap();
    apply(
        &mut driver,
        json!({"@type":"upgradedGift","@extra":bad_quote.as_extra(),"name":null}),
    );
    assert!(!driver.session.marketplace_gift.as_ref().unwrap().loading);
    assert!(
        driver
            .session
            .marketplace_gift
            .as_ref()
            .unwrap()
            .quote
            .is_none()
    );
}
