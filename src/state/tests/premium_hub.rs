//! State reducer tests: Stars history, received gifts, Premium explainer.
use super::common::*;
use super::*;

fn tx_page(extra: u64, id: &str) -> String {
    format!(
        r#"{{"@type":"starTransactions","@extra":"{extra}","star_amount":{{"@type":"starAmount","star_count":300,"nanostar_count":0}},"next_offset":"n","transactions":[{{"@type":"starTransaction","id":"{id}","star_amount":{{"@type":"starAmount","star_count":-5,"nanostar_count":0}},"is_refund":false,"date":1,"type":{{"@type":"starTransactionTypeGiftPurchase","owner_id":{{"@type":"messageSenderUser","user_id":2}},"gift":{{"@type":"gift","star_count":5}}}}}}]}}"#
    )
}

#[test]
fn transactions_apply_only_to_the_newest_request() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let old = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetStarTransactions { append: false }),
        None,
    );
    let new = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetStarTransactions { append: false }),
        None,
    );
    session.payments.hub.tx_request = new.0;
    session.payments.hub.tx_loading = true;
    // A slower answer to the previous filter is dropped.
    apply_json(&mut session, &seq, &sink, &tx_page(old.0, "stale"));
    assert!(session.payments.hub.transactions.is_empty());
    apply_json(&mut session, &seq, &sink, &tx_page(new.0, "fresh"));
    assert_eq!(session.payments.hub.transactions.len(), 1);
    assert_eq!(session.payments.hub.transactions[0].id, "fresh");
    assert_eq!(session.payments.hub.balance.map(|b| b.stars), Some(300));
    assert!(!session.payments.hub.tx_loading);
    assert_eq!(session.payments.hub.tx_offset, "n");
}

#[test]
fn owned_star_count_updates_the_balance() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateOwnedStarCount","star_amount":{"@type":"starAmount","star_count":77,"nanostar_count":0}}"#,
    );
    assert_eq!(session.payments.hub.balance.map(|b| b.stars), Some(77));
}

#[test]
fn gifts_page_applies_and_registers_sticker_files() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetReceivedGifts { append: false }),
        None,
    );
    session.payments.hub.gifts_request = extra.0;
    session.payments.hub.gifts_loading = true;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"receivedGifts","@extra":"{}","total_count":1,"next_offset":"","are_notifications_enabled":true,"gifts":[{{"received_gift_id":"g1","is_saved":true,"date":5,"sell_star_count":10,"gift":{{"@type":"sentGiftRegular","gift":{{"@type":"gift","star_count":12,"sticker":{{"@type":"sticker","id":1,"emoji":"x","width":512,"height":512,"format":{{"@type":"stickerFormatWebp"}},"sticker":{{"@type":"file","id":88,"size":1,"expected_size":1,"local":{{"path":"","is_downloading_completed":false,"can_be_downloaded":true}},"remote":{{"id":"r","unique_id":"u"}}}}}}}}}}}}]}}"#,
            extra.0
        ),
    );
    assert_eq!(session.payments.hub.gifts.len(), 1);
    assert!(session.payments.hub.gifts_loaded && !session.payments.hub.gifts_loading);
    assert!(session.files.contains_key(&88));
}

#[test]
fn gift_mutation_ok_marks_the_list_stale_and_errors_release_the_lock() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.payments.hub.gift_mutating = true;
    let extra = session.request(
        RequestPurpose::Payments(PaymentsPurpose::ToggleGiftSaved { saved: false }),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(!session.payments.hub.gift_mutating);
    assert!(session.payments.hub.gifts_stale);

    session.payments.hub.gift_mutating = true;
    session.payments.hub.gift_convert_confirm = Some("g1".into());
    let extra = session.request(RequestPurpose::SellGift, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"STARGIFT_CONVERT_TOO_OLD"}}"#,
            extra.0
        ),
    );
    assert!(!session.payments.hub.gift_mutating);
    assert!(session.payments.hub.gift_convert_confirm.is_none());
    assert!(session.payments.hub.gifts_error.is_some());
}

#[test]
fn premium_features_apply_to_their_request() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetPremiumFeatures, None);
    session.payments.hub.premium_loading = true;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"premiumFeatures","@extra":"{}","features":[{{"@type":"premiumFeatureDisabledAds"}}],"limits":[]}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session
            .payments
            .hub
            .premium
            .as_ref()
            .map(|p| p.features.len()),
        Some(1)
    );
    assert!(!session.payments.hub.premium_loading);
}
