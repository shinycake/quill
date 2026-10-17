//! Connect-driver tests: a group's bot commands come with its full info.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn count_sent(recorder: &Arc<RecordingSender>, type_name: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| json.contains(&format!("\"@type\":\"{type_name}\"")))
        .count()
}

#[test]
fn opening_a_basic_group_fetches_its_full_info_once() {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink;
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    for json in [
        r#"{"@type":"updateNewChat","chat":{"id":-1001,"title":"Basic","type":{"@type":"chatTypeBasicGroup","basic_group_id":1},"unread_count":0}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":22,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":22},"unread_count":0}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    // A chat with a person never asks for a group's full info.
    driver.select_chat(ChatId(22)).unwrap();
    assert_eq!(count_sent(&recorder, "getBasicGroupFullInfo"), 0);

    driver.select_chat(ChatId(-1001)).unwrap();
    assert_eq!(count_sent(&recorder, "getBasicGroupFullInfo"), 1);
    // Opening it again while the answer is pending sends nothing new.
    driver.select_chat(ChatId(-1001)).unwrap();
    assert_eq!(count_sent(&recorder, "getBasicGroupFullInfo"), 1);
}

#[test]
fn unknown_bots_in_a_group_are_asked_for_once_and_fill_the_menu() {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink;
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    for json in [
        r#"{"@type":"updateNewChat","chat":{"id":-1002,"title":"Mega","type":{"@type":"chatTypeSupergroup","supergroup_id":2,"is_channel":false},"unread_count":0}}"#,
        r#"{"@type":"updateSupergroupFullInfo","supergroup_id":2,"supergroup_full_info":{"description":"","bot_commands":[{"@type":"botCommands","bot_user_id":77,"commands":[{"@type":"botCommand","command":"go","description":"","is_ephemeral":false}]}]}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    assert!(driver.session.command_menu_items(ChatId(-1002)).is_empty());
    assert!(driver.group_bot_users_tick());
    assert_eq!(count_sent(&recorder, "getUser"), 1);
    // Asked once: another tick sends nothing, even with the answer pending.
    assert!(!driver.group_bot_users_tick());
    assert_eq!(count_sent(&recorder, "getUser"), 1);
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateUser","user":{"id":77,"first_name":"Go","usernames":{"@type":"usernames","active_usernames":["gobot"],"disabled_usernames":[],"editable_username":"gobot"},"type":{"@type":"userTypeBot","is_inline":false}}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    let rows = driver.session.command_menu_items(ChatId(-1002));
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows.first().map(|row| row.command_text(true)),
        Some("/go@gobot".to_string())
    );
}
