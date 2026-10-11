//! The "Choose members" list and the giveaway limits (recorded JSON,
//! nothing live).
use super::super::*;
use super::support::{prepared_tmp, ready_driver, sent_request};
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Driver = ConnectDriver<Arc<RecordingSender>>;

fn parts() -> (
    Driver,
    Arc<RecordingSender>,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
) {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &sink, &seq);
    (driver, recorder, sink, seq)
}

fn feed(driver: &mut Driver, seq: &AtomicU64, sink: &Arc<dyn DiagnosticSink>, value: Value) {
    driver
        .ingest(copy_and_parse(&value.to_string(), seq, sink).unwrap())
        .unwrap();
}

fn with_channel(driver: &mut Driver, seq: &AtomicU64, sink: &Arc<dyn DiagnosticSink>) {
    feed(
        driver,
        seq,
        sink,
        json!({"@type":"updateNewChat","chat":{"id":-1001,"title":"Channel",
            "type":{"@type":"chatTypeSupergroup","supergroup_id":1001,"is_channel":true}}}),
    );
}

fn member(user_id: i64) -> Value {
    json!({"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":user_id},
        "status":{"@type":"chatMemberStatusMember"}})
}

#[test]
fn members_load_a_page_at_a_time_and_a_search_asks_the_server() {
    let (mut driver, recorder, sink, seq) = parts();
    with_channel(&mut driver, &seq, &sink);
    driver.session.my_user_id = Some(2);
    driver.search_award_members(ChatId(-1001), "").unwrap();
    let first = sent_request(&recorder, "getSupergroupMembers");
    assert_eq!(first["filter"]["@type"], "supergroupMembersFilterRecent");
    assert_eq!(first["offset"], 0);
    assert_eq!(first["supergroup_id"], 1001);
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"chatMembers","@extra":first["@extra"],"total_count":4,
            "members":[member(1), member(2), member(3)]}),
    );
    // Yourself is left out; the offset counts what the server sent.
    let list = &driver.session.payments.giveaway.award_members;
    assert_eq!(list.user_ids, vec![1, 3]);
    assert_eq!(list.offset, 3);
    assert!(!list.done);
    // The next page continues where the last one ended.
    driver.load_more_award_members().unwrap().unwrap();
    let next = sent_request(&recorder, "getSupergroupMembers");
    assert_eq!(next["offset"], 3);
    assert_eq!(driver.load_more_award_members().unwrap(), None);
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"chatMembers","@extra":next["@extra"],"total_count":4,
            "members":[member(4)]}),
    );
    let list = &driver.session.payments.giveaway.award_members;
    assert_eq!(list.user_ids, vec![1, 3, 4]);
    assert!(list.done);
    assert_eq!(driver.load_more_award_members().unwrap(), None);
    // A typed name is searched on the server and starts the list over.
    driver.search_award_members(ChatId(-1001), " ada ").unwrap();
    let search = sent_request(&recorder, "getSupergroupMembers");
    assert_eq!(search["filter"]["@type"], "supergroupMembersFilterSearch");
    assert_eq!(search["filter"]["query"], "ada");
    assert_eq!(search["offset"], 0);
    assert!(
        driver
            .session
            .payments
            .giveaway
            .award_members
            .user_ids
            .is_empty()
    );
    // The answer to the earlier list no longer counts.
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"chatMembers","@extra":next["@extra"],"total_count":4,
            "members":[member(9)]}),
    );
    assert!(
        driver
            .session
            .payments
            .giveaway
            .award_members
            .user_ids
            .is_empty()
    );
}

#[test]
fn a_chat_that_is_not_a_channel_is_refused() {
    let (mut driver, recorder, _sink, _seq) = parts();
    assert_eq!(
        driver.search_award_members(ChatId(-5), ""),
        Err(ConnectSendError::InvalidRequest)
    );
    assert!(
        recorder
            .snapshot()
            .iter()
            .all(|j| !j.contains("getSupergroupMembers"))
    );
    assert!(!driver.session.payments.giveaway.award_members.loading);
}

#[test]
fn the_giveaway_limits_follow_updateoption() {
    let (mut driver, _recorder, sink, seq) = parts();
    let limits = driver.session.payments.giveaway.limits;
    assert_eq!(limits.chats_max, 10);
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"updateOption","name":"giveaway_additional_chat_count_max",
            "value":{"@type":"optionValueInteger","value":"25"}}),
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"updateOption","name":"giveaway_country_count_max",
            "value":{"@type":"optionValueInteger","value":"5"}}),
    );
    let limits = driver.session.payments.giveaway.limits;
    assert_eq!((limits.chats_max, limits.countries_max), (25, 5));
}
