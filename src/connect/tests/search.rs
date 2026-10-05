//! Connect-driver tests: global and chat search, shared media.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::state::{SearchStatus, Session};
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

#[test]
fn driver_search_happy_empty_error_and_select() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    assert_eq!(
        driver.set_search_query("alice"),
        Err(ConnectSendError::InvalidRequest)
    );
    let seq = AtomicU64::new(0);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    let extras = commit_typed_search(&mut driver, "alice");
    let SearchFlight::Query(chats_extra, messages_extra, public_extra) = extras else {
        panic!("expected typed search");
    };
    let sent = recorder.snapshot();
    assert!(
        sent.iter()
            .any(|j| j.contains("\"@type\":\"searchChats\"") && j.contains("alice"))
    );
    assert!(sent.iter().any(|j| {
        j.contains("\"@type\":\"searchMessages\"") && j.contains("\"chat_list\":null")
    }));
    assert!(
        sent.iter()
            .any(|j| { j.contains("\"@type\":\"searchPublicChats\"") && j.contains("alice") })
    );
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[7]}}"#,
                    chats_extra.0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
                    public_extra.0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"foundMessages","@extra":"{}","total_count":1,"next_offset":"","messages":[{{"id":50,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_DRV_search","entities":[]}}}}}}]}}"#,
                        messages_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(driver.session.search.status, SearchStatus::Ready);
    assert_eq!(driver.session.search.chat_ids, vec![ChatId(7)]);
    driver
        .select_search_message(ChatId(7), MessageId(50), "", None, 0)
        .unwrap();
    assert_eq!(driver.session.search.status, SearchStatus::Closed);
    assert_eq!(driver.session.open_chat, Some(ChatId(7)));
    // The hit opens in context: a window loads around it (highlighted)
    // instead of the lone hit being spliced into the history.
    assert_eq!(
        driver.session.chat_search.jump,
        crate::state::ChatSearchJump::Loading {
            message_id: MessageId(50)
        }
    );
    assert!(recorder.snapshot().iter().any(|j| {
        j.contains("\"@type\":\"getChatHistory\"") && j.contains("\"from_message_id\":50")
    }));
    assert!(
        recorder.snapshot().iter().any(
            |j| j.contains("\"@type\":\"addRecentlyFoundChat\"") && j.contains("\"chat_id\":7")
        )
    );
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("\"@type\":\"openChat\"") && j.contains("\"chat_id\":7"))
    );

    let recents = driver.open_search().unwrap().expect("recents");
    let SearchFlight::Recents(recents_extra) = recents else {
        panic!("expected recents");
    };
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("\"@type\":\"searchRecentlyFoundChats\""))
    );
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[7]}}"#,
                    recents_extra.0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(driver.session.search.status, SearchStatus::Ready);
    assert!(driver.session.search.recents);

    let empty = commit_typed_search(&mut driver, "zzz");
    let SearchFlight::Query(empty_chats, empty_messages, empty_public) = empty else {
        panic!("expected typed empty search");
    };
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
                    empty_chats.0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"foundMessages","@extra":"{}","total_count":0,"next_offset":"","messages":[]}}"#,
                        empty_messages.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    // Phase 7.2: `Empty` only once the public search settles too.
    assert_eq!(driver.session.search.status, SearchStatus::Searching);
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
                    empty_public.0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(driver.session.search.status, SearchStatus::Empty);

    let fail = commit_typed_search(&mut driver, "nope");
    let SearchFlight::Query(fail_chats, fail_messages, fail_public) = fail else {
        panic!("expected typed fail search");
    };
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"error","code":400,"message":"CANARY_DRV_ERR","@extra":"{}"}}"#,
                    fail_chats.0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"error","code":400,"message":"CANARY_DRV_ERR2","@extra":"{}"}}"#,
                    fail_messages.0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    // Phase 7.2: the public request still in flight keeps `Searching`.
    assert_eq!(driver.session.search.status, SearchStatus::Searching);
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"error","code":400,"message":"CANARY_DRV_ERR3","@extra":"{}"}}"#,
                    fail_public.0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(driver.session.search.status, SearchStatus::Failed);
    driver.close_search();
    assert_eq!(driver.session.search.status, SearchStatus::Closed);
    assert!(!sink.rendered().contains("CANARY_DRV"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_typed_search_debounce_settles_once() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    let t1 = match driver.set_search_query("a").unwrap() {
        SearchQueryOutcome::Debounced { token } => token,
        other => panic!("{other:?}"),
    };
    let t2 = match driver.set_search_query("al").unwrap() {
        SearchQueryOutcome::Debounced { token } => token,
        other => panic!("{other:?}"),
    };
    let t3 = match driver.set_search_query("alice").unwrap() {
        SearchQueryOutcome::Debounced { token } => token,
        other => panic!("{other:?}"),
    };
    assert_ne!(t1, t3);
    assert_ne!(t2, t3);
    assert!(
        !recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("\"@type\":\"searchChats\""))
    );
    assert!(driver.commit_debounced_search(t1).unwrap().is_none());
    assert!(driver.commit_debounced_search(t2).unwrap().is_none());
    assert!(
        !recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("\"@type\":\"searchChats\""))
    );
    let settled = driver
        .commit_debounced_search(t3)
        .unwrap()
        .expect("settled");
    let SearchFlight::Query(_, _, _) = settled else {
        panic!("expected typed pair");
    };
    let sent: Vec<String> = recorder
        .snapshot()
        .into_iter()
        .filter(|j| {
            j.contains("\"@type\":\"searchChats\"") || j.contains("\"@type\":\"searchMessages\"")
        })
        .collect();
    assert_eq!(sent.len(), 2);
    assert!(sent.iter().all(|j| j.contains("alice")));
    assert!(
        !sent
            .iter()
            .any(|j| j.contains("\"query\":\"a\"") || j.contains("\"query\":\"al\""))
    );
    assert_eq!(SEARCH_DEBOUNCE, Duration::from_millis(900));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_chat_search_debounce_jump_empty_and_close() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    assert_eq!(
        driver.set_chat_search_query("hello"),
        Err(ConnectSendError::InvalidRequest)
    );
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    assert!(driver.open_chat_search().unwrap());
    assert!(driver.session.chat_search.open);

    let t1 = match driver.set_chat_search_query("h").unwrap() {
        ChatSearchQueryOutcome::Debounced { token } => token,
        other => panic!("{other:?}"),
    };
    let t2 = match driver.set_chat_search_query("he").unwrap() {
        ChatSearchQueryOutcome::Debounced { token } => token,
        other => panic!("{other:?}"),
    };
    let t3 = match driver.set_chat_search_query("hello").unwrap() {
        ChatSearchQueryOutcome::Debounced { token } => token,
        other => panic!("{other:?}"),
    };
    assert!(
        !recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("\"@type\":\"searchChatMessages\""))
    );
    assert!(driver.commit_debounced_chat_search(t1).unwrap().is_none());
    assert!(driver.commit_debounced_chat_search(t2).unwrap().is_none());
    let ChatSearchFlight::Query(extra) = driver
        .commit_debounced_chat_search(t3)
        .unwrap()
        .expect("settled");
    let sent = recorder.snapshot();
    let search_json = sent
        .iter()
        .rev()
        .find(|j| j.contains("\"@type\":\"searchChatMessages\""))
        .expect("searchChatMessages");
    let v: Value = serde_json::from_str(search_json).unwrap();
    assert_eq!(v["query"], "hello");
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["topic_id"], Value::Null);
    assert_eq!(v["sender_id"], Value::Null);
    assert_eq!(v["from_message_id"], 0);
    assert_eq!(v["offset"], 0);
    assert_eq!(v["limit"], CHAT_SEARCH_LIMIT);
    assert_eq!(v["filter"], Value::Null);
    assert!(!search_json.contains("\"query\":\"h\""));

    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":40,"messages":[{{"id":50,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_DRV_chat","entities":[]}}}}}},{{"id":40,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older hello","entities":[]}}}}}}]}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(driver.session.chat_search.status, SearchStatus::Ready);
    assert_eq!(
        driver.session.chat_search.jump,
        crate::state::ChatSearchJump::Ready {
            message_id: MessageId(50)
        }
    );
    let around = driver
        .jump_to_chat_search_message(MessageId(40))
        .unwrap()
        .expect("around load");
    let around_json = recorder
        .snapshot()
        .into_iter()
        .rev()
        .find(|j| {
            j.contains("\"@type\":\"getChatHistory\"") && j.contains("\"from_message_id\":40")
        })
        .expect("getChatHistory around");
    let around_v: Value = serde_json::from_str(&around_json).unwrap();
    assert_eq!(around_v["offset"], HISTORY_AROUND_OFFSET);
    assert_eq!(around_v["limit"], HISTORY_AROUND_LIMIT);
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{{"id":40,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older hello","entities":[]}}}}}},{{"id":39,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"neighbor","entities":[]}}}}}}]}}"#,
                        around.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(
        driver.session.chat_search.jump,
        crate::state::ChatSearchJump::Ready {
            message_id: MessageId(40)
        }
    );
    assert!(
        driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .contains(MessageId(39))
    );

    let ChatSearchFlight::Query(empty_extra) = commit_typed_chat_search(&mut driver, "zzz");
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":0,"next_from_message_id":0,"messages":[]}}"#,
                        empty_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(driver.session.chat_search.status, SearchStatus::Empty);

    // Jumping to 40 replaced the window (50 was newer and not adjacent to
    // the loaded page): it now knows it stops short of the latest message.
    let history = driver.session.histories.get(&7).unwrap();
    assert!(!history.contains(MessageId(50)));
    assert!(history.has_newer);
    driver.close_chat_search();
    assert_eq!(driver.session.chat_search.status, SearchStatus::Closed);
    assert_eq!(driver.session.open_chat, Some(ChatId(7)));
    assert!(
        driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .contains(MessageId(40))
    );
    assert_eq!(SEARCH_DEBOUNCE, Duration::from_millis(900));
    assert!(!sink.rendered().contains("CANARY_DRV_chat"));
    let _ = std::fs::remove_dir_all(&dir);
}
