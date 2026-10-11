//! Connect-driver tests: shared-folder new chats, folder "mark as read",
//! `getPremiumLimit` for the limit boxes.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::folder_limits::FolderLimitKind;
use crate::ids::AccountKey;
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn folder_json(id: i32, shared: bool) -> String {
    format!(
        r#"{{"@type":"chatFolderInfo","id":{id},"name":{{"@type":"chatFolderName","text":{{"@type":"formattedText","text":"F{id}","entities":[]}}}},"icon":null,"color_id":-1,"is_shareable":{shared},"has_my_invite_links":false}}"#
    )
}

#[test]
fn driver_new_chats_folder_read_and_premium_limit() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    // Not ready: nothing is sent.
    assert_eq!(
        driver.fetch_folder_new_chats(5),
        Err(ConnectSendError::InvalidRequest)
    );
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    let folders = format!(
        r#"{{"@type":"updateChatFolders","chat_folders":[{},{}],"main_chat_list_position":0,"are_tags_enabled":false}}"#,
        folder_json(5, true),
        folder_json(6, false)
    );
    driver
        .ingest(copy_and_parse(&folders, &seq, &dyn_sink).unwrap())
        .unwrap();

    // A folder that is not shared never asks for new chats.
    assert_eq!(driver.fetch_folder_new_chats(6), Ok(None));
    let extra = driver
        .fetch_folder_new_chats(5)
        .expect("send")
        .expect("request");
    let last: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
    assert_eq!(last["@type"], "getChatFolderNewChats");
    assert_eq!(last["chat_folder_id"], 5);
    // One call per update period: in flight, then too soon.
    assert_eq!(driver.fetch_folder_new_chats(5), Ok(None));
    let answer = format!(
        r#"{{"@type":"chats","@extra":"{}","total_count":2,"chat_ids":[41,42]}}"#,
        extra.0
    );
    driver
        .ingest(copy_and_parse(&answer, &seq, &dyn_sink).unwrap())
        .unwrap();
    assert_eq!(driver.session.chat_list.folder_new_chats[&5], vec![41, 42]);
    assert_eq!(driver.fetch_folder_new_chats(5), Ok(None));

    // Joining sends the chosen chats and drops the bar at once.
    driver.process_folder_new_chats(5, &[41]).expect("process");
    let last: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
    assert_eq!(last["@type"], "processChatFolderNewChats");
    assert_eq!(last["added_chat_ids"], serde_json::json!([41]));
    assert!(!driver.session.chat_list.folder_new_chats.contains_key(&5));

    // Nothing unread in folder 5: no `readChatList`.
    assert_eq!(driver.mark_folder_as_read(5), Ok(None));

    // The premium limit is asked once.
    driver
        .fetch_premium_limit(FolderLimitKind::Folders)
        .expect("send")
        .expect("request");
    let last: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
    assert_eq!(last["@type"], "getPremiumLimit");
    assert_eq!(
        last["limit_type"]["@type"],
        "premiumLimitTypeChatFolderCount"
    );
    assert_eq!(
        driver.fetch_premium_limit(FolderLimitKind::Folders),
        Ok(None)
    );
    assert_eq!(driver.fetch_premium_limit(FolderLimitKind::Tags), Ok(None));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_marks_a_folder_as_read_only_when_something_is_unread() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    let chat = r#"{"@type":"updateNewChat","chat":{"id":77,"title":"Unread one","type":{"@type":"chatTypePrivate","user_id":77},"unread_count":3,"positions":[{"@type":"chatPosition","list":{"@type":"chatListFolder","chat_folder_id":5},"order":"9","is_pinned":false}]}}"#;
    driver
        .ingest(copy_and_parse(chat, &seq, &dyn_sink).unwrap())
        .unwrap();
    driver
        .mark_folder_as_read(5)
        .expect("send")
        .expect("request");
    let last: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
    assert_eq!(last["@type"], "readChatList");
    assert_eq!(last["chat_list"]["@type"], "chatListFolder");
    assert_eq!(last["chat_list"]["chat_folder_id"], 5);
    // Another folder has nothing unread.
    assert_eq!(driver.mark_folder_as_read(9), Ok(None));
    let _ = std::fs::remove_dir_all(&dir);
}
