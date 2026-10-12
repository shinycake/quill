//! Driver tests: every send into a paid chat goes through the guard, the
//! trusted price is stored per account, and a paid supergroup gates like a
//! user. TDLib JSON is recorded; no Stars are sent.
use super::super::*;
use super::paid_messages::{Fixture, cleanup, extra_of, fixture, ingest, open_private, requests};
use crate::ids::ChatId;
use crate::paid_messages::{CanSend, PaidTrust, SendGate};

/// A paid private chat 600 (a private chat has the id of its user) with the guard told its price.
fn paid_chat(f: &mut Fixture, stars: i64) {
    open_private(f, 600, 600, false, false);
    f.1.maybe_fetch_paid_messages_info().unwrap();
    let ask = requests(f, "canSendMessageToUser").remove(0);
    ingest(
        f,
        &format!(
            r#"{{"@type":"canSendMessageToUserResultUserHasPaidMessages","outgoing_paid_message_star_count":{stars},"@extra":"{}"}}"#,
            extra_of(&ask)
        ),
    );
}

fn sticker(chat: i64, extra: u64) -> String {
    format!(
        r#"{{"@type":"sendMessage","@extra":"{extra}","chat_id":{chat},"options":{{"@type":"messageSendOptions","paid_message_star_count":0}},"input_message_content":{{"@type":"inputMessageSticker"}}}}"#
    )
}

#[test]
fn a_sticker_into_a_paid_chat_waits_for_the_confirmation() {
    let mut f = fixture();
    paid_chat(&mut f, 25);
    f.1.sender.send_json(&sticker(600, 90)).unwrap();
    assert!(requests(&f, "sendMessage").is_empty(), "nothing went out");
    let held = f.1.held_paid_send().expect("held");
    assert_eq!((held.per_message, held.messages, held.total()), (25, 1, 25));
    // Cancel: gone, and nothing is ever sent.
    f.1.discard_paid_send();
    assert!(f.1.held_paid_send().is_none());
    assert!(requests(&f, "sendMessage").is_empty());
    // Confirm: it goes out carrying the total.
    f.1.sender.send_json(&sticker(600, 91)).unwrap();
    f.1.release_paid_send(false).unwrap();
    let sent = requests(&f, "sendMessage");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["options"]["paid_message_star_count"], 25);
    assert!(f.1.held_paid_send().is_none());
    cleanup(f);
}

#[test]
fn forwards_carry_price_times_messages() {
    let mut f = fixture();
    paid_chat(&mut f, 10);
    let forward = r#"{"@type":"forwardMessages","@extra":"92","chat_id":600,"from_chat_id":5,"message_ids":[1,2,3],"options":{"@type":"messageSendOptions","paid_message_star_count":0}}"#;
    f.1.sender.send_json(forward).unwrap();
    assert_eq!(f.1.held_paid_send().unwrap().total(), 30);
    f.1.release_paid_send(false).unwrap();
    assert_eq!(
        requests(&f, "forwardMessages")[0]["options"]["paid_message_star_count"],
        30
    );
    cleanup(f);
}

#[test]
fn a_remembered_price_is_stored_and_lets_sends_through() {
    let mut f = fixture();
    paid_chat(&mut f, 10);
    f.1.remember_paid_trust(ChatId(600), 10);
    let file = f.1.paths.root.join("paid_trust.json");
    let read = || PaidTrust::from_json(&std::fs::read_to_string(&file).unwrap());
    assert!(read().is_trusted(600, 10));
    f.1.sender.send_json(&sticker(600, 93)).unwrap();
    assert!(f.1.held_paid_send().is_none());
    assert_eq!(
        requests(&f, "sendMessage")[0]["options"]["paid_message_star_count"],
        10
    );
    // The user stops charging: the stored price goes away too.
    f.1.session
        .payments
        .paid_messages
        .record_answer(600, CanSend::Ok, 1);
    f.1.sync_paid_guard();
    assert!(read().entries().is_empty());
    cleanup(f);
}

#[test]
fn a_paid_supergroup_gates_and_holds_like_a_user() {
    let mut f = fixture();
    ingest(
        &mut f,
        r#"{"@type":"updateNewChat","chat":{"id":-1009,"title":"Direct","type":{"@type":"chatTypeSupergroup","supergroup_id":900,"is_channel":false},"unread_count":0}}"#,
    );
    ingest(
        &mut f,
        r#"{"@type":"updateSupergroupFullInfo","supergroup_id":900,"supergroup_full_info":{"@type":"supergroupFullInfo","outgoing_paid_message_star_count":40}}"#,
    );
    assert_eq!(
        f.1.session.send_gate(ChatId(-1009)),
        SendGate::Charges { stars: 40 }
    );
    f.1.sender.send_json(&sticker(-1009, 94)).unwrap();
    assert_eq!(f.1.held_paid_send().unwrap().per_message, 40);
    f.1.discard_paid_send();
    cleanup(f);
}

#[test]
fn the_direct_messages_price_is_fetched_when_unknown() {
    let mut f = fixture();
    ingest(
        &mut f,
        r#"{"@type":"updateNewChat","chat":{"id":-1001,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":100,"is_channel":true},"unread_count":0}}"#,
    );
    ingest(
        &mut f,
        r#"{"@type":"updateSupergroupFullInfo","supergroup_id":100,"supergroup_full_info":{"@type":"supergroupFullInfo","direct_messages_chat_id":-1002}}"#,
    );
    // The group's chat is not loaded: its chat is asked for first.
    f.1.fetch_direct_messages_price(ChatId(-1001)).unwrap();
    assert_eq!(requests(&f, "getChat")[0]["chat_id"], -1002);
    // Loaded: the supergroup is asked for, once.
    ingest(
        &mut f,
        r#"{"@type":"updateNewChat","chat":{"id":-1002,"title":"News DM","type":{"@type":"chatTypeSupergroup","supergroup_id":200,"is_channel":false},"unread_count":0}}"#,
    );
    f.1.fetch_direct_messages_price(ChatId(-1001)).unwrap();
    f.1.fetch_direct_messages_price(ChatId(-1001)).unwrap();
    let asks = requests(&f, "getSupergroup");
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0]["supergroup_id"], 200);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"supergroup","id":200,"is_direct_messages_group":true,"paid_message_star_count":30,"@extra":"{}"}}"#,
            extra_of(&asks[0])
        ),
    );
    assert_eq!(
        f.1.session.payments.paid_messages.dm_price.get(&200),
        Some(&30)
    );
    // Known now: no further request.
    f.1.fetch_direct_messages_price(ChatId(-1001)).unwrap();
    assert_eq!(requests(&f, "getSupergroup").len(), 1);
    cleanup(f);
}
