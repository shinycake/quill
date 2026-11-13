//! State reducer tests: Stars history, received gifts, Premium explainer.
use super::common::*;
use super::*;
use crate::premium_hub::gift_view::GiftView;
use crate::premium_hub::page_stage::{PageKind, PageStage};

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
    session.payments.hub.stars.page = PageStage::Loading {
        request: new,
        kind: PageKind::First,
    };
    // A slower answer to the previous filter is dropped.
    apply_json(&mut session, &seq, &sink, &tx_page(old.0, "stale"));
    assert!(session.payments.hub.stars.rows.is_empty());
    apply_json(&mut session, &seq, &sink, &tx_page(new.0, "fresh"));
    assert_eq!(session.payments.hub.stars.rows.len(), 1);
    assert_eq!(session.payments.hub.stars.rows[0].id, "fresh");
    assert_eq!(session.payments.hub.balance.map(|b| b.stars), Some(300));
    assert!(!session.payments.hub.stars.page.is_loading());
    assert_eq!(session.payments.hub.stars.offset, "n");
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
    session.payments.hub.received.page = PageStage::Loading {
        request: extra,
        kind: PageKind::First,
    };
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"receivedGifts","@extra":"{}","total_count":1,"next_offset":"","are_notifications_enabled":true,"gifts":[{{"received_gift_id":"g1","is_saved":true,"date":5,"sell_star_count":10,"gift":{{"@type":"sentGiftRegular","gift":{{"@type":"gift","star_count":12,"sticker":{{"@type":"sticker","id":1,"emoji":"x","width":512,"height":512,"format":{{"@type":"stickerFormatWebp"}},"sticker":{{"@type":"file","id":88,"size":1,"expected_size":1,"local":{{"path":"","is_downloading_completed":false,"can_be_downloaded":true}},"remote":{{"id":"r","unique_id":"u"}}}}}}}}}}}}]}}"#,
            extra.0
        ),
    );
    assert_eq!(session.payments.hub.received.rows.len(), 1);
    assert!(
        session.payments.hub.received.page.is_loaded()
            && !session.payments.hub.received.page.is_loading()
    );
    assert!(session.media.files.contains_key(&88));
}

#[test]
fn gift_mutation_ok_marks_the_list_stale_and_errors_release_the_lock() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(
        RequestPurpose::Payments(PaymentsPurpose::ToggleGiftSaved { saved: false }),
        None,
    );
    session.payments.hub.received.open();
    session.payments.hub.received.op = crate::state::fetch::ListOp::Mutating(extra);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(!session.payments.hub.received.op.is_mutating());
    assert!(session.payments.hub.received.op.is_stale());

    session.payments.hub.received.view.select(Some("g1".into()));
    session
        .payments
        .hub
        .received
        .view
        .confirm_convert(Some("g1".into()));
    let extra = session.request_for_subject(RequestPurpose::SellGift, "g1");
    session.payments.hub.received.op = crate::state::fetch::ListOp::Mutating(extra);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"STARGIFT_CONVERT_TOO_OLD"}}"#,
            extra.0
        ),
    );
    assert!(!session.payments.hub.received.op.is_mutating());
    assert_eq!(session.payments.hub.received.view.confirming(), None);
    assert_eq!(
        session.payments.hub.received.view.selected(),
        Some("g1"),
        "the panel stays"
    );
    assert!(session.payments.hub.received.notice.is_some());
}

#[test]
fn another_requests_answer_does_not_free_a_gift_mutation() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let busy = session.request(RequestPurpose::SellGift, None);
    session.payments.hub.received.op = crate::state::fetch::ListOp::Mutating(busy);
    let other = session.request(
        RequestPurpose::Payments(PaymentsPurpose::ToggleGiftSaved { saved: true }),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, other.0),
    );
    assert!(
        session.payments.hub.received.op.is_mutating(),
        "only the request that locked it frees it"
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, busy.0),
    );
    assert!(!session.payments.hub.received.op.is_mutating());
}

#[test]
fn a_sold_gifts_answer_leaves_another_selected_gift_open() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.payments.hub.received.open();
    let sell = |session: &mut Session, id: &str| {
        session.payments.hub.received.view.select(Some("g2".into()));
        session
            .payments
            .hub
            .received
            .view
            .confirm_convert(Some("g2".into()));
        session.request_for_subject(RequestPurpose::SellGift, id)
    };
    let extra = sell(&mut session, "g1");
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert_eq!(session.payments.hub.received.view.selected(), Some("g2"));
    assert_eq!(session.payments.hub.received.view.confirming(), Some("g2"));
    let extra = sell(&mut session, "g2");
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert_eq!(session.payments.hub.received.view, GiftView::List);
}

#[test]
fn premium_features_apply_to_their_request() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetPremiumFeatures, None);
    session.payments.hub.premium.begin_with(extra);
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
            .data()
            .map(|p| p.features.len()),
        Some(1)
    );
    assert!(!session.payments.hub.premium.is_loading());
}

#[test]
fn business_features_keep_the_server_order() {
    use crate::premium_hub::BusinessFeature;
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetBusinessFeatures, None);
    session.payments.hub.business_features.begin_with(extra);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"businessFeatures","@extra":"{}","features":[{{"@type":"businessFeatureQuickReplies"}},{{"@type":"businessFeatureEmojiStatus"}},{{"@type":"businessFeatureLocation"}}]}}"#,
            extra.0
        ),
    );
    assert!(!session.payments.hub.business_features.is_loading());
    // The emoji status row is not one tdesktop lists.
    assert_eq!(
        session.payments.hub.business_features.data(),
        Some(&vec![
            BusinessFeature::QuickReplies,
            BusinessFeature::Location
        ])
    );
}

#[test]
fn a_late_answer_of_an_older_gift_change_leaves_the_newer_one_locked() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let old = session.request(
        RequestPurpose::Payments(PaymentsPurpose::ToggleGiftSaved { saved: true }),
        None,
    );
    let new = session.request(RequestPurpose::SellGift, None);
    session.payments.hub.received.op = crate::state::fetch::ListOp::Mutating(new);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, old.0),
    );
    assert!(
        session.payments.hub.received.op.is_mutating(),
        "the sale is still out"
    );
}

/// 04-027: the Premium state is its own fetch: only its request's answer
/// lands, its error is kept (and asked again), and a change of Premium
/// status makes it out of date.
#[test]
fn the_premium_state_is_matched_to_its_request_and_refreshes() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let old = session.request(RequestPurpose::GetPremiumState, None);
    let live = session.request(RequestPurpose::GetPremiumState, None);
    session.payments.hub.premium_state.begin_with(live);
    let state = |extra: u64, text: &str| {
        format!(
            r#"{{"@type":"premiumState","@extra":"{extra}","state":{{"text":"{text}","entities":[]}},"payment_options":[],"animations":[],"business_animations":[]}}"#
        )
    };
    apply_json(&mut session, &seq, &sink, &state(old.0, "old"));
    assert!(session.payments.hub.premium_state.is_loading());
    apply_json(&mut session, &seq, &sink, &state(live.0, "live"));
    assert_eq!(
        session
            .payments
            .hub
            .premium_state
            .data()
            .map(|s| s.text.as_str()),
        Some("live")
    );
    // A failure of the request out is kept for the next opening to retry.
    let failing = session.request(RequestPurpose::GetPremiumState, None);
    session.payments.hub.premium_state.begin_with(failing);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"INTERNAL"}}"#,
            failing.0
        ),
    );
    assert!(session.payments.hub.premium_state.can_ask());
    assert!(session.payments.hub.premium_state.error().is_some());
    // Subscribing changes the status the plans were asked under.
    session
        .payments
        .hub
        .premium_state
        .succeed(Default::default());
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateOption","name":"is_premium","value":{"@type":"optionValueBoolean","value":true}}"#,
    );
    assert!(session.payments.hub.premium_state.is_outdated());
}
