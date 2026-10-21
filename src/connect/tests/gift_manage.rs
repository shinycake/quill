//! Connect-driver tests: received-gift management and sending gifts
//! (recorded JSON, nothing live, no Stars move).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::premium_hub::StarAmount;
use crate::state::{PaymentsPurpose, RequestPurpose};
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::MessageSender;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Recorder = Arc<RecordingSender>;
type Parts = (
    ConnectDriver<Recorder>,
    Recorder,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
);

fn parts() -> Parts {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.session.my_user_id = Some(7);
    (driver, recorder, sink, seq)
}

fn feed(p: &mut Parts, json: Value) {
    let (driver, _, sink, seq) = p;
    driver
        .ingest(copy_and_parse(&json.to_string(), seq, sink).unwrap())
        .unwrap();
}

fn count(recorder: &RecordingSender, type_name: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| serde_json::from_str::<Value>(json).is_ok_and(|v| v["@type"] == type_name))
        .count()
}

fn received(id: &str, extra: Value) -> Value {
    let mut gift = json!({
        "@type": "receivedGift", "received_gift_id": id, "date": 5, "is_saved": true,
        "gift": {"@type": "sentGiftUpgraded", "gift": {
            "@type": "upgradedGift", "title": "Plush Pepe", "number": 4}},
    });
    for (k, v) in extra.as_object().unwrap() {
        gift[k] = v.clone();
    }
    gift
}

fn regular(id: &str, extra: Value) -> Value {
    let mut gift = json!({
        "@type": "receivedGift", "received_gift_id": id, "date": 5, "is_saved": true,
        "can_be_upgraded": true,
        "gift": {"@type": "sentGiftRegular", "gift": {
            "@type": "gift", "id": "9", "star_count": 100, "upgrade_star_count": 30}},
    });
    for (k, v) in extra.as_object().unwrap() {
        gift[k] = v.clone();
    }
    gift
}

/// Open my own gifts list and answer it with `gifts`.
fn open_my_gifts(p: &mut Parts, gifts: Vec<Value>) {
    let extra =
        p.0.open_received_gifts(MessageSender::User { user_id: 7 })
            .unwrap();
    let total = gifts.len();
    feed(
        p,
        json!({"@type": "receivedGifts", "@extra": extra.as_extra(), "total_count": total,
            "next_offset": "", "gifts": gifts}),
    );
}

fn add_chat(p: &mut Parts, json: Value) {
    feed(p, json);
}

#[test]
fn opening_my_gifts_loads_collections_and_limits_once() {
    let mut p = parts();
    open_my_gifts(&mut p, vec![received("a", json!({}))]);
    assert_eq!(count(&p.1, "getGiftCollections"), 1);
    assert_eq!(count(&p.1, "getOption"), 5);
    let sent = sent_request(&p.1, "getGiftCollections");
    assert_eq!(sent["owner_id"]["user_id"], 7);
    feed(
        &mut p,
        json!({"@type": "giftCollections", "@extra": sent["@extra"], "collections": [
            {"@type": "giftCollection", "id": 4, "name": "Cats", "gift_count": 1}]}),
    );
    assert_eq!(p.0.session.payments.gifts_manage.collections.len(), 1);
    // The limit options land by name.
    let option =
        p.1.snapshot()
            .iter()
            .filter_map(|j| serde_json::from_str::<Value>(j).ok())
            .find(|v| v["name"] == "pinned_gift_count_max")
            .unwrap();
    feed(
        &mut p,
        json!({"@type": "optionValueInteger", "@extra": option["@extra"], "value": "3"}),
    );
    assert_eq!(p.0.session.payments.gifts_manage.limits.pinned(), 3);
    // Reopening does not ask for the options again.
    open_my_gifts(&mut p, vec![]);
    assert_eq!(count(&p.1, "getOption"), 5);
}

#[test]
fn pinning_sends_the_whole_order_and_respects_the_limit() {
    let mut p = parts();
    open_my_gifts(
        &mut p,
        vec![
            received("a", json!({"is_pinned": true})),
            received("b", json!({})),
            regular("r", json!({})),
        ],
    );
    // A regular gift cannot be pinned.
    assert!(p.0.set_gift_pinned("r", true).is_err());
    // Already pinned: cannot be pinned again.
    assert!(p.0.set_gift_pinned("a", true).is_err());
    let extra = p.0.set_gift_pinned("b", true).unwrap();
    let sent = sent_request(&p.1, "setPinnedGifts");
    assert_eq!(sent["received_gift_ids"], json!(["a", "b"]));
    // One mutation at a time.
    assert!(p.0.set_gift_pinned("a", false).is_err());
    feed(&mut p, json!({"@type": "ok", "@extra": extra.as_extra()}));
    assert!(!p.0.session.payments.hub.gift_mutating);
    // The list refetches from the server (never optimistic).
    assert_eq!(count(&p.1, "getReceivedGifts"), 2);
    // At the limit a new pin is refused, an unpin still works.
    p.0.session.payments.gifts_manage.limits.pinned_max = Some(1);
    assert!(p.0.set_gift_pinned("b", true).is_err());
    p.0.set_gift_pinned("a", false).unwrap();
    assert_eq!(
        sent_request(&p.1, "setPinnedGifts")["received_gift_ids"],
        json!([])
    );
}

#[test]
fn pinning_is_refused_inside_a_collection() {
    let mut p = parts();
    open_my_gifts(&mut p, vec![received("b", json!({}))]);
    let sent = sent_request(&p.1, "getGiftCollections");
    feed(
        &mut p,
        json!({"@type": "giftCollections", "@extra": sent["@extra"], "collections": [
            {"@type": "giftCollection", "id": 4, "name": "Cats", "gift_count": 1}]}),
    );
    p.0.select_gift_collection(4).unwrap();
    assert_eq!(sent_request(&p.1, "getReceivedGifts")["collection_id"], 4);
    let extra = sent_request(&p.1, "getReceivedGifts")["@extra"].clone();
    feed(
        &mut p,
        json!({"@type": "receivedGifts", "@extra": extra, "total_count": 1,
            "next_offset": "", "gifts": [received("b", json!({"collection_ids": [4]}))]}),
    );
    assert!(p.0.set_gift_pinned("b", true).is_err());
    // Unknown collections cannot be selected.
    assert!(p.0.select_gift_collection(99).is_err());
    assert!(p.0.select_gift_collection(4).unwrap().is_none());
}

#[test]
fn upgrade_costs_are_taken_from_the_gift_and_balance_is_checked() {
    let mut p = parts();
    open_my_gifts(
        &mut p,
        vec![
            regular("r", json!({})),
            regular("prepaid", json!({"prepaid_upgrade_star_count": 30})),
            received("u", json!({})),
        ],
    );
    // Not upgradable: a collectible.
    assert!(p.0.upgrade_received_gift("u", false).is_err());
    // Too poor.
    p.0.session.payments.hub.balance = Some(StarAmount {
        stars: 10,
        nanos: 0,
    });
    assert!(p.0.upgrade_received_gift("r", false).is_err());
    assert_eq!(count(&p.1, "upgradeGift"), 0);
    // Prepaid upgrades are free whatever the balance.
    let extra = p.0.upgrade_received_gift("prepaid", true).unwrap();
    let sent = sent_request(&p.1, "upgradeGift");
    assert_eq!(sent["star_count"], 0);
    assert_eq!(sent["keep_original_details"], true);
    feed(
        &mut p,
        json!({"@type": "upgradeGiftResult", "@extra": extra.as_extra(),
            "gift": {"@type": "upgradedGift", "title": "Plush Pepe", "number": 12}}),
    );
    let manage = &p.0.session.payments.gifts_manage;
    assert_eq!(manage.note.as_deref(), Some("Upgraded to Plush Pepe #12."));
    assert!(!p.0.session.payments.hub.gift_mutating);
    // Enough Stars: the paid upgrade sends the gift's price.
    p.0.session.payments.hub.balance = Some(StarAmount {
        stars: 50,
        nanos: 0,
    });
    p.0.upgrade_received_gift("r", false).unwrap();
    assert_eq!(sent_request(&p.1, "upgradeGift")["star_count"], 30);
}

#[test]
fn transfer_needs_a_readable_recipient_and_the_gifts_price() {
    let mut p = parts();
    add_chat(
        &mut p,
        json!({"@type":"updateNewChat","chat":{"id":7,"title":"Saved Messages","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}),
    );
    add_chat(
        &mut p,
        json!({"@type":"updateNewChat","chat":{"id":21,"title":"Ann","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}),
    );
    add_chat(
        &mut p,
        json!({"@type":"updateNewChat","chat":{"id":-100,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":100,"is_channel":true},"unread_count":0}}),
    );
    add_chat(
        &mut p,
        json!({"@type":"updateNewChat","chat":{"id":-200,"title":"Group","type":{"@type":"chatTypeBasicGroup","basic_group_id":200},"unread_count":0}}),
    );
    open_my_gifts(
        &mut p,
        vec![
            received(
                "u",
                json!({"can_be_transferred": true, "transfer_star_count": 25}),
            ),
            received(
                "locked",
                json!({"can_be_transferred": true, "next_transfer_date": 2_000_000_000_i64}),
            ),
            received("fixed", json!({"can_be_transferred": false})),
        ],
    );
    // Not to myself, not to a group, not for a locked or fixed gift.
    assert!(p.0.transfer_received_gift("u", ChatId(7)).is_err());
    assert!(p.0.transfer_received_gift("u", ChatId(-200)).is_err());
    assert!(p.0.transfer_received_gift("locked", ChatId(21)).is_err());
    assert!(p.0.transfer_received_gift("fixed", ChatId(21)).is_err());
    p.0.session.payments.hub.balance = Some(StarAmount { stars: 5, nanos: 0 });
    assert!(p.0.transfer_received_gift("u", ChatId(21)).is_err());
    assert_eq!(count(&p.1, "transferGift"), 0);
    p.0.session.payments.hub.balance = Some(StarAmount {
        stars: 25,
        nanos: 0,
    });
    let extra = p.0.transfer_received_gift("u", ChatId(-100)).unwrap();
    let sent = sent_request(&p.1, "transferGift");
    assert_eq!(sent["new_owner_id"]["@type"], "messageSenderChat");
    assert_eq!(sent["new_owner_id"]["chat_id"], -100);
    assert_eq!(sent["star_count"], 25);
    feed(&mut p, json!({"@type": "ok", "@extra": extra.as_extra()}));
    let hub = &p.0.session.payments.hub;
    assert!(!hub.gift_mutating && hub.gift_selected.is_none());
}

#[test]
fn resale_price_must_be_in_range_and_removal_needs_a_listing() {
    let mut p = parts();
    open_my_gifts(
        &mut p,
        vec![
            received("sale", json!({"next_resale_date": 10})),
            received("none", json!({})),
            received(
                "listed",
                json!({"next_resale_date": 10, "gift": {"@type": "sentGiftUpgraded",
                "gift": {"@type": "upgradedGift", "title": "X", "number": 1,
                    "resale_parameters": {"star_count": 500, "gram_only": false}}}}),
            ),
        ],
    );
    assert!(
        p.0.set_received_gift_resale_price("none", Some(500))
            .is_err()
    );
    assert!(
        p.0.set_received_gift_resale_price("sale", Some(10))
            .is_err()
    );
    assert!(p.0.set_received_gift_resale_price("sale", None).is_err());
    let extra =
        p.0.set_received_gift_resale_price("sale", Some(500))
            .unwrap();
    let sent = sent_request(&p.1, "setGiftResalePrice");
    assert_eq!(sent["price"]["star_count"], 500);
    feed(&mut p, json!({"@type": "ok", "@extra": extra.as_extra()}));
    p.0.set_received_gift_resale_price("listed", None).unwrap();
    assert!(sent_request(&p.1, "setGiftResalePrice")["price"].is_null());
}

#[test]
fn collections_are_created_renamed_filled_and_deleted() {
    let mut p = parts();
    open_my_gifts(
        &mut p,
        vec![
            received("a", json!({"collection_ids": [4]})),
            received("b", json!({})),
        ],
    );
    let list = sent_request(&p.1, "getGiftCollections");
    feed(
        &mut p,
        json!({"@type": "giftCollections", "@extra": list["@extra"], "collections": [
            {"@type": "giftCollection", "id": 4, "name": "Cats", "gift_count": 1}]}),
    );
    assert!(p.0.create_gift_collection("   ").is_err());
    assert!(p.0.create_gift_collection("far too long a name").is_err());
    let extra = p.0.create_gift_collection(" Dogs ").unwrap();
    assert_eq!(sent_request(&p.1, "createGiftCollection")["name"], "Dogs");
    let before = count(&p.1, "getGiftCollections");
    feed(
        &mut p,
        json!({"@type": "giftCollection", "@extra": extra.as_extra(), "id": 5, "name": "Dogs", "gift_count": 0}),
    );
    // The answer marks both lists stale; the same ingest refetches them.
    assert!(!p.0.session.payments.gifts_manage.collections_stale);
    assert_eq!(count(&p.1, "getGiftCollections"), before + 1);

    // Add only when absent, remove only when present.
    assert!(p.0.change_gift_collection(4, "a", true).is_err());
    assert!(p.0.change_gift_collection(4, "b", false).is_err());
    assert!(p.0.change_gift_collection(99, "b", true).is_err());
    let extra = p.0.change_gift_collection(4, "b", true).unwrap();
    let sent = sent_request(&p.1, "addGiftCollectionGifts");
    assert_eq!(sent["collection_id"], 4);
    assert_eq!(sent["received_gift_ids"], json!(["b"]));
    feed(
        &mut p,
        json!({"@type": "giftCollection", "@extra": extra.as_extra(), "id": 4, "name": "Cats", "gift_count": 2}),
    );
    p.0.change_gift_collection(4, "a", false).unwrap();
    assert_eq!(count(&p.1, "removeGiftCollectionGifts"), 1);
    p.0.session.payments.hub.gift_mutating = false;

    assert!(p.0.rename_gift_collection(4, "").is_err());
    p.0.rename_gift_collection(4, "Kittens").unwrap();
    assert_eq!(
        sent_request(&p.1, "setGiftCollectionName")["name"],
        "Kittens"
    );
    p.0.session.payments.hub.gift_mutating = false;
    assert!(p.0.delete_gift_collection(99).is_err());
    let extra = p.0.delete_gift_collection(4).unwrap();
    feed(&mut p, json!({"@type": "ok", "@extra": extra.as_extra()}));
    assert_eq!(p.0.session.payments.gifts_manage.active_collection, 0);
}

#[test]
fn deleting_a_gift_sends_delete_gift_and_removes_it_on_ok() {
    let mut p = parts();
    open_my_gifts(
        &mut p,
        vec![
            regular("a", json!({"can_be_deleted": true})),
            regular("b", json!({})),
            received("c", json!({"can_be_deleted": true})),
        ],
    );
    p.0.session.payments.hub.gift_selected = Some("a".into());
    // Only a gift TDLib marks deletable, and only one that is listed.
    assert!(p.0.delete_received_gift("b").is_err());
    assert!(p.0.delete_received_gift("zzz").is_err());
    assert_eq!(count(&p.1, "deleteGift"), 0);
    let extra = p.0.delete_received_gift("a").unwrap();
    let sent = sent_request(&p.1, "deleteGift");
    assert_eq!(sent["received_gift_id"], "a");
    assert!(p.0.session.payments.hub.gift_mutating);
    // Nothing is optimistic: the gift stays until the answer.
    assert_eq!(p.0.session.payments.hub.gifts.len(), 3);
    feed(&mut p, json!({"@type": "ok", "@extra": extra.as_extra()}));
    let hub = &p.0.session.payments.hub;
    assert!(!hub.gift_mutating);
    assert_eq!(
        hub.gifts.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(),
        ["b", "c"]
    );
    assert_eq!(hub.gifts_total, 2);
    assert_eq!(hub.gift_selected, None);
    assert_eq!(
        p.0.session.payments.gifts_manage.note.as_deref(),
        Some("Gift Deleted")
    );
}

#[test]
fn a_failed_delete_keeps_the_gift_and_says_why() {
    let mut p = parts();
    open_my_gifts(&mut p, vec![regular("a", json!({"can_be_deleted": true}))]);
    let extra = p.0.delete_received_gift("a").unwrap();
    feed(
        &mut p,
        json!({"@type": "error", "@extra": extra.as_extra(), "code": 400, "message": "SAVED_ID_INVALID"}),
    );
    let hub = &p.0.session.payments.hub;
    assert!(!hub.gift_mutating);
    assert_eq!(hub.gifts.len(), 1);
    assert_eq!(p.0.session.payments.gifts_manage.deleting, None);
    assert!(
        p.0.session
            .payments
            .gifts_manage
            .note
            .as_deref()
            .is_some_and(|n| n.starts_with("Couldn't delete the gift"))
    );
}

#[test]
fn deleting_is_refused_on_other_peoples_gifts() {
    let mut p = parts();
    let extra =
        p.0.open_received_gifts(MessageSender::User { user_id: 99 })
            .unwrap();
    feed(
        &mut p,
        json!({"@type": "receivedGifts", "@extra": extra.as_extra(), "total_count": 1,
            "next_offset": "", "gifts": [regular("a", json!({"can_be_deleted": true}))]}),
    );
    assert!(p.0.delete_received_gift("a").is_err());
}

#[test]
fn owner_actions_are_refused_on_other_peoples_gifts() {
    let mut p = parts();
    let extra =
        p.0.open_received_gifts(MessageSender::User { user_id: 99 })
            .unwrap();
    feed(
        &mut p,
        json!({"@type": "receivedGifts", "@extra": extra.as_extra(), "total_count": 1,
            "next_offset": "", "gifts": [received("a", json!({"can_be_transferred": true,
                "next_resale_date": 10}))]}),
    );
    assert!(p.0.set_gift_pinned("a", true).is_err());
    assert!(p.0.create_gift_collection("Mine").is_err());
    assert!(p.0.set_received_gift_resale_price("a", Some(500)).is_err());
}

fn open_send_dialog(p: &mut Parts) {
    add_chat(
        p,
        json!({"@type":"updateNewChat","chat":{"id":21,"title":"Ann","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}),
    );
    p.0.open_send_gift(ChatId(21)).unwrap();
    let sent = sent_request(&p.1, "getAvailableGifts");
    feed(
        p,
        json!({"@type": "availableGifts", "@extra": sent["@extra"], "gifts": [
            {"@type": "availableGift", "resale_count": 0, "gift": {
                "@type": "gift", "id": "55", "star_count": 50, "upgrade_star_count": 20}},
            {"@type": "availableGift", "resale_count": 0, "gift": {
                "@type": "gift", "id": "56", "star_count": 90, "upgrade_star_count": 0,
                "overall_limits": {"remaining_count": 0, "total_count": 10}}},
        ]}),
    );
}

#[test]
fn sending_a_gift_needs_an_open_dialog_a_sendable_gift_and_enough_stars() {
    let mut p = parts();
    // Nothing is requested for a group or before the dialog is open.
    assert!(p.0.send_star_gift(55, "", false, false).is_err());
    add_chat(
        &mut p,
        json!({"@type":"updateNewChat","chat":{"id":-200,"title":"Group","type":{"@type":"chatTypeBasicGroup","basic_group_id":200},"unread_count":0}}),
    );
    assert!(p.0.open_send_gift(ChatId(-200)).is_err());
    assert_eq!(count(&p.1, "getAvailableGifts"), 0);

    open_send_dialog(&mut p);
    let send = &p.0.session.payments.gifts_manage.send;
    assert!(send.open && send.gifts_loaded && send.gifts.len() == 2);
    assert_eq!(count(&p.1, "getPremiumGiftPaymentOptions"), 1);
    // A sold-out gift and an unknown gift are refused.
    assert!(p.0.send_star_gift(56, "", false, false).is_err());
    assert!(p.0.send_star_gift(1, "", false, false).is_err());
    // The upgrade is only allowed when the gift has an upgrade price.
    assert!(p.0.send_star_gift(56, "", false, true).is_err());
    // Not enough Stars for the gift plus the upgrade.
    p.0.session.payments.hub.balance = Some(StarAmount {
        stars: 60,
        nanos: 0,
    });
    assert!(p.0.send_star_gift(55, "", false, true).is_err());
    assert_eq!(count(&p.1, "sendGift"), 0);
    // A comment needs the length limit.
    assert!(p.0.send_star_gift(55, "hello", false, false).is_err());
    p.0.session.payments.gift_text_length_max = Some(255);
    assert!(
        p.0.send_star_gift(55, &"x".repeat(256), false, false)
            .is_err()
    );

    let extra = p.0.send_star_gift(55, " hello ", true, false).unwrap();
    let sent = sent_request(&p.1, "sendGift");
    assert_eq!(sent["gift_id"], "55");
    assert_eq!(sent["owner_id"]["user_id"], 21);
    assert_eq!(sent["text"]["text"], "hello");
    assert_eq!(sent["is_private"], true);
    assert_eq!(sent["pay_for_upgrade"], false);
    // No double submit while it is in flight.
    assert!(p.0.send_star_gift(55, "", false, false).is_err());
    assert_eq!(count(&p.1, "sendGift"), 1);
    feed(&mut p, json!({"@type": "ok", "@extra": extra.as_extra()}));
    let send = &p.0.session.payments.gifts_manage.send;
    assert!(!send.sending);
    assert_eq!(send.done.as_deref(), Some("Gift sent."));
}

#[test]
fn a_failed_send_releases_the_dialog_and_says_so() {
    let mut p = parts();
    open_send_dialog(&mut p);
    let extra = p.0.send_star_gift(55, "", false, false).unwrap();
    feed(
        &mut p,
        json!({"@type": "error", "@extra": extra.as_extra(), "code": 400, "message": "STARGIFT_USAGE_LIMITED"}),
    );
    let send = &p.0.session.payments.gifts_manage.send;
    assert!(!send.sending && send.done.is_none());
    assert!(
        send.error
            .as_deref()
            .is_some_and(|e| e.starts_with("Couldn't send the gift"))
    );
    assert!(p.0.send_star_gift(55, "", false, false).is_ok());
}

#[test]
fn bots_and_self_are_handled_as_recipients() {
    let mut p = parts();
    add_chat(
        &mut p,
        json!({"@type":"updateNewChat","chat":{"id":7,"title":"Saved Messages","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}),
    );
    let me = p.0.session.gift_recipient(ChatId(7)).unwrap();
    assert!(me.is_self && !me.is_channel);
    // Gifts to myself have no Premium option request.
    p.0.open_send_gift(ChatId(7)).unwrap();
    assert_eq!(count(&p.1, "getPremiumGiftPaymentOptions"), 0);
    let purpose = RequestPurpose::Payments(PaymentsPurpose::GetAvailableGifts);
    assert!(p.0.session.requests.has_purpose(purpose));
}
