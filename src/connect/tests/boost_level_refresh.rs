//! A channel's boost level moving in `updateSupergroup` makes the driver ask
//! for the boost status again; an unchanged level sends nothing.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::state::RequestPurpose;
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn supergroup_update(level: i32) -> String {
    format!(
        r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":113,"is_channel":true,"boost_level":{level},"status":{{"@type":"chatMemberStatusMember"}}}}}}"#
    )
}

#[test]
fn a_moved_boost_level_refetches_the_status() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &sink).unwrap())
            .unwrap();
    };
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":113,"is_channel":true},"unread_count":0}}"#,
    );
    ingest(&mut driver, &supergroup_update(1));
    driver.session.groups.set_boost_status(
        ChatId(13).0,
        crate::telegram::envelope::ParsedBoostStatus {
            level: 1,
            boost_count: 4,
            ..Default::default()
        },
    );
    let asked = |recorder: &RecordingSender| {
        recorder
            .snapshot()
            .iter()
            .filter(|json| json.contains(r#""@type":"getChatBoostStatus""#))
            .count()
    };

    ingest(&mut driver, &supergroup_update(1));
    assert_eq!(asked(&recorder), 0);

    ingest(&mut driver, &supergroup_update(2));
    assert_eq!(asked(&recorder), 1);
    // The held status stays visible while the new one is on its way.
    let held = driver.session.groups.chat_boost_status.get(&13);
    assert!(held.is_some_and(|fetch| fetch.is_loading()));
    assert_eq!(driver.session.groups.boost_level(13), Some(1));
}

/// r2-14, 02-075: a status request asked before the level moved is dropped
/// and asked again, so its old answer is not cached as the current one.
#[test]
fn a_status_in_flight_when_the_level_moves_is_replaced() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &sink).unwrap())
            .unwrap();
    };
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":113,"is_channel":true},"unread_count":0}}"#,
    );
    ingest(&mut driver, &supergroup_update(1));
    driver.session.groups.set_boost_status(
        ChatId(13).0,
        crate::telegram::envelope::ParsedBoostStatus {
            level: 1,
            boost_count: 4,
            ..Default::default()
        },
    );
    let early = driver
        .session
        .request(RequestPurpose::GetChatBoostStatus, Some(ChatId(13)));
    ingest(&mut driver, &supergroup_update(2));
    assert!(
        driver.session.requests.get(early).is_none(),
        "the request asked before the move is dropped"
    );
    assert!(
        driver
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatBoostStatus, ChatId(13)),
        "a new one is out"
    );
}
