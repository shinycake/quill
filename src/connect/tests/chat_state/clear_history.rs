//! Driver tests: "Clear history" sends `deleteChatHistory` with the revoke
//! flag the confirmation box chose, and keeps the chat in the list.
use super::*;

fn driver_with_chat(
    recorder: &Arc<ViewCtlSender>,
    dir_store: &MemorySecretStore,
) -> (ConnectDriver<Arc<ViewCtlSender>>, std::path::PathBuf) {
    let (dir, prepared) = prepared_tmp(dir_store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    for json in [
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Old","type":{"@type":"chatTypePrivate","user_id":9},"unread_count":0,"can_be_deleted_only_for_self":true,"can_be_deleted_for_all_users":true}}"#,
    ] {
        let Some(envelope) = copy_and_parse(json, &seq, &dyn_sink) else {
            panic!("fixture does not parse");
        };
        let Ok(_) = driver.ingest(envelope) else {
            panic!("fixture not ingested");
        };
    }
    (driver, dir)
}

fn sent_history_delete(recorder: &Arc<ViewCtlSender>) -> Value {
    let Some(delete) = recorder
        .snapshot()
        .into_iter()
        .find(|j| j.contains("deleteChatHistory"))
    else {
        panic!("deleteChatHistory not sent");
    };
    let Ok(value) = serde_json::from_str(&delete) else {
        panic!("request is not JSON");
    };
    value
}

#[test]
fn clearing_for_the_user_alone_does_not_revoke() {
    let store = MemorySecretStore::new();
    let recorder = ViewCtlSender::new();
    let (mut driver, dir) = driver_with_chat(&recorder, &store);
    assert!(matches!(
        driver.clear_chat_history(ChatId(9), false),
        Ok(Some(_))
    ));
    let value = sent_history_delete(&recorder);
    assert_eq!(value["chat_id"], 9);
    assert_eq!(value["remove_from_chat_list"], false);
    assert_eq!(value["revoke"], false);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_checkbox_clears_for_everyone() {
    let store = MemorySecretStore::new();
    let recorder = ViewCtlSender::new();
    let (mut driver, dir) = driver_with_chat(&recorder, &store);
    assert!(matches!(
        driver.clear_chat_history(ChatId(9), true),
        Ok(Some(_))
    ));
    let value = sent_history_delete(&recorder);
    assert_eq!(value["remove_from_chat_list"], false);
    assert_eq!(value["revoke"], true);
    let _ = std::fs::remove_dir_all(&dir);
}
