//! Connect-driver tests: the info panel's members list asks for its next
//! page from the right offset, once at a time.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::state::{MemberListFilter, SupergroupMembersFetch};
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn members_json(ids: std::ops::Range<i64>, total: i32, extra: &str) -> String {
    let members: Vec<String> = ids
        .map(|id| {
            format!(
                r#"{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":{id}}},"tag":"","inviter_user_id":0,"joined_chat_date":0,"status":{{"@type":"chatMemberStatusMember"}}}}"#
            )
        })
        .collect();
    format!(
        r#"{{"@type":"chatMembers","@extra":"{extra}","total_count":{total},"members":[{}]}}"#,
        members.join(",")
    )
}

#[test]
fn next_page_goes_out_at_the_loaded_offset_and_appends() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let feed = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &sink).unwrap())
            .unwrap();
    };
    feed(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"G","type":{"@type":"chatTypeSupergroup","supergroup_id":113,"is_channel":false},"unread_count":0}}"#,
    );
    let sent = |recorder: &Arc<RecordingSender>| -> Vec<Value> {
        recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("getSupergroupMembers"))
            .map(|j| serde_json::from_str(j).unwrap())
            .collect()
    };

    // Nothing is loaded yet: no next page.
    assert_eq!(
        driver
            .fetch_supergroup_members_more(ChatId(13), MemberListFilter::Recent, "", 0)
            .unwrap(),
        None
    );
    driver
        .fetch_supergroup_members(ChatId(13), MemberListFilter::Recent, "")
        .unwrap();
    let first = sent(&recorder).pop().unwrap();
    assert_eq!(first["offset"], 0);
    let extra = first["@extra"].as_str().unwrap().to_string();
    feed(&mut driver, &members_json(1..4, 9, &extra));

    // The wrong offset is refused; the loaded count is the offset.
    assert_eq!(
        driver
            .fetch_supergroup_members_more(ChatId(13), MemberListFilter::Recent, "", 5)
            .unwrap(),
        None
    );
    let more = driver
        .fetch_supergroup_members_more(ChatId(13), MemberListFilter::Recent, "", 3)
        .unwrap()
        .expect("next page sent");
    let request = sent(&recorder).pop().unwrap();
    assert_eq!(request["offset"], 3);
    assert_eq!(request["limit"], 200);
    assert_eq!(request["supergroup_id"], 113);
    assert_eq!(request["filter"]["@type"], "supergroupMembersFilterRecent");
    // One at a time.
    assert!(driver.supergroup_members_more_in_flight(ChatId(13), MemberListFilter::Recent));
    assert_eq!(
        driver
            .fetch_supergroup_members_more(ChatId(13), MemberListFilter::Recent, "", 3)
            .unwrap(),
        None
    );

    feed(&mut driver, &members_json(4..7, 9, &more.as_extra()));
    assert!(!driver.supergroup_members_more_in_flight(ChatId(13), MemberListFilter::Recent));
    let Some(SupergroupMembersFetch::Loaded {
        members,
        total_count,
    }) = driver
        .session
        .groups
        .supergroup_members
        .get(&(13, MemberListFilter::Recent))
    else {
        panic!("page dropped");
    };
    assert_eq!((members.len(), *total_count), (6, 9));
    drop(dir);
}
