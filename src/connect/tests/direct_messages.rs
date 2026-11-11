use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use crate::telegram::requests::SendReply;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const DIRECT: i64 = -1_000_000_000_082;

fn sent(recorder: &RecordingSender, ty: &str) -> Vec<Value> {
    recorder
        .snapshot()
        .into_iter()
        .filter_map(|j| serde_json::from_str::<Value>(&j).ok())
        .filter(|v| v["@type"] == ty)
        .collect()
}

fn topic_json(id: i64) -> String {
    format!(
        r#"{{"@type":"updateDirectMessagesChatTopic","topic":{{"@type":"directMessagesChatTopic","chat_id":{DIRECT},"id":"{id}","sender_id":{{"@type":"messageSenderUser","user_id":{id}}},"order":"{id}","can_send_unpaid_messages":false,"is_marked_as_unread":false,"unread_count":2,"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"unread_reaction_count":0,"last_message":{{"id":{last},"chat_id":{DIRECT},"date":1,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}},"draft_message":null}}}}"#,
        last = id * 1_000,
    )
}

#[test]
fn an_admin_loads_conversations_and_writes_into_one() {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        if let Some(owned) = copy_and_parse(json, &seq, &dyn_sink) {
            let _ = driver.ingest(owned);
        }
    };
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{DIRECT},"title":"Quill Weekly","type":{{"@type":"chatTypeSupergroup","supergroup_id":82,"is_channel":false}},"unread_count":0,"permissions":{{"can_send_basic_messages":true}},"positions":[{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"1","is_pinned":false}}]}}}}"#
        ),
    );
    assert!(sent(&recorder, "loadDirectMessagesChatTopics").is_empty());
    ingest(
        &mut driver,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":82,"is_channel":false,"is_direct_messages_group":true,"is_administered_direct_messages_group":true,"status":{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_manage_direct_messages":true}}}}"#,
    );
    let loads = sent(&recorder, "loadDirectMessagesChatTopics");
    assert_eq!(
        loads.len(),
        1,
        "a listed admin chat loads its conversations"
    );
    assert_eq!(loads[0]["chat_id"], DIRECT);
    ingest(&mut driver, &topic_json(501));
    ingest(&mut driver, &topic_json(502));

    driver.select_chat(ChatId(DIRECT)).unwrap();
    assert!(
        sent(&recorder, "getChatHistory")
            .iter()
            .any(|v| v["chat_id"] == DIRECT),
        "All is the chat's own history"
    );
    driver
        .select_direct_topic(ChatId(DIRECT), Some(502))
        .unwrap();
    let pages = sent(&recorder, "getDirectMessagesChatTopicHistory");
    let page = pages.last().unwrap();
    assert_eq!(page["topic_id"], 502);

    let routed = driver.thread_routed(
        ChatId(DIRECT),
        crate::telegram::requests::send_text(
            crate::ids::RequestId(9),
            ChatId(DIRECT),
            driver.send_topic(ChatId(DIRECT)),
            "hello",
            None,
            &crate::composer::SendOptions::default(),
        ),
    );
    let routed: Value = serde_json::from_str(&routed).unwrap();
    assert_eq!(routed["topic_id"]["direct_messages_chat_topic_id"], 502);

    driver.select_direct_topic(ChatId(DIRECT), None).unwrap();
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":7000,"chat_id":{DIRECT},"date":2,"is_outgoing":false,"topic_id":{{"@type":"messageTopicDirectMessages","direct_messages_chat_topic_id":501}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"q","entities":[]}}}}}}}}"#
        ),
    );
    let reply = SendReply {
        message_id: MessageId(7000),
        quote: None,
        source_chat: None,
        item: Default::default(),
    };
    let send = crate::telegram::requests::send_text(
        crate::ids::RequestId(10),
        ChatId(DIRECT),
        driver.send_topic(ChatId(DIRECT)),
        "answer",
        Some(reply),
        &crate::composer::SendOptions::default(),
    );
    let routed: Value = serde_json::from_str(&driver.thread_routed(ChatId(DIRECT), send)).unwrap();
    assert_eq!(routed["topic_id"]["@type"], "messageTopicDirectMessages");
    assert_eq!(routed["topic_id"]["direct_messages_chat_topic_id"], 501);
}

#[test]
fn a_failed_first_page_is_asked_again_when_the_chat_opens() {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        if let Some(owned) = copy_and_parse(json, &seq, &dyn_sink) {
            let _ = driver.ingest(owned);
        }
    };
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{DIRECT},"title":"Quill Weekly","type":{{"@type":"chatTypeSupergroup","supergroup_id":82,"is_channel":false}},"unread_count":0,"permissions":{{"can_send_basic_messages":true}},"positions":[{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"1","is_pinned":false}}]}}}}"#
        ),
    );
    let admin = r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":82,"is_channel":false,"is_direct_messages_group":true,"is_administered_direct_messages_group":true,"status":{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_manage_direct_messages":true}}}}"#;
    ingest(&mut driver, admin);
    let loads = sent(&recorder, "loadDirectMessagesChatTopics");
    assert_eq!(loads.len(), 1);
    let extra = loads[0]["@extra"].clone();
    ingest(
        &mut driver,
        &format!(r#"{{"@type":"error","@extra":{extra},"code":500,"message":"Internal"}}"#),
    );
    assert!(!driver.session.threads.direct.preloaded(DIRECT));
    // Another update about the chat does not ask again.
    ingest(&mut driver, admin);
    assert_eq!(sent(&recorder, "loadDirectMessagesChatTopics").len(), 1);
    // Opening the chat does.
    driver.select_chat(ChatId(DIRECT)).unwrap();
    assert_eq!(sent(&recorder, "loadDirectMessagesChatTopics").len(), 2);
    assert!(driver.session.threads.direct.preloaded(DIRECT));
}
