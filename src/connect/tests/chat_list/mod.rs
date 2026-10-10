//! Connect-driver tests: folders, channel admin, bots.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::state::{RequestPurpose, Session};
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

mod contacts_bots;
mod open_close;

#[test]
fn driver_load_folder_chats_pages_chat_list_folder() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    assert_eq!(
        driver.load_folder_chats(2),
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

    let extra = driver.load_folder_chats(2).expect("folder load");
    let sent = recorder.snapshot();
    let folder_load = sent
        .iter()
        .rev()
        .find(|j| j.contains("chatListFolder"))
        .expect("loadChats(chatListFolder)");
    let v: Value = serde_json::from_str(folder_load).unwrap();
    assert_eq!(v["@type"], "loadChats");
    assert_eq!(v["chat_list"]["@type"], "chatListFolder");
    assert_eq!(v["chat_list"]["chat_folder_id"], 2);
    assert_eq!(v["limit"], MAIN_CHAT_LOAD_LIMIT);
    assert_eq!(v["@extra"].as_str().unwrap(), extra.0.to_string());
    assert!(
        driver
            .session
            .requests
            .has_purpose_for_folder(RequestPurpose::LoadFolderChats, 2)
    );

    // Parity slice: the ok response pages on (folder "load more") — a
    // second `loadChats(chatListFolder)` goes out, not a main-list page.
    let loads_before = recorder
        .snapshot()
        .iter()
        .filter(|j| j.contains("loadChats"))
        .count();
    driver
        .ingest(
            copy_and_parse(
                &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    let loads: Vec<String> = recorder
        .snapshot()
        .iter()
        .filter(|j| j.contains("loadChats"))
        .cloned()
        .collect();
    assert_eq!(loads.len(), loads_before + 1);
    assert!(loads.last().unwrap().contains("chatListFolder"));
    assert!(!loads.last().unwrap().contains("chatListMain"));
    let second_extra = driver
        .session
        .requests
        .pending_extra_for_folder(RequestPurpose::LoadFolderChats, 2)
        .expect("second page in flight");

    // A 404 marks the folder exhausted: the next ok pages no further.
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"error","@extra":"{}","code":404,"message":"not found"}}"#,
                    second_extra.0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(driver.session.folder_chats_exhausted.contains(&2));
    assert_eq!(
        driver.maybe_load_folder_chats(2).unwrap(),
        None,
        "exhausted folder pages no more"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_create_chat_folder_sends_create_chat_folder() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    let mut editor = crate::folders::FolderEditor::new();
    editor.name = "Work".to_string();
    editor.include_groups = true;
    editor.toggle_included(7);
    let spec = editor.to_spec();
    let extra = driver.create_chat_folder(&spec).expect("create");
    let json = recorder.snapshot().last().cloned().expect("sent");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "createChatFolder");
    assert_eq!(v["@extra"].as_str().unwrap(), extra.0.to_string());
    assert_eq!(v["folder"]["@type"], "chatFolder");
    assert_eq!(v["folder"]["name"]["text"]["text"], "Work");
    assert_eq!(v["folder"]["included_chat_ids"], serde_json::json!([7]));
    assert_eq!(v["folder"]["include_groups"], true);
    assert!(
        driver
            .session
            .requests
            .has_purpose(RequestPurpose::CreateChatFolder)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_edit_and_delete_folder_flow() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatFolders","chat_folders":[{"@type":"chatFolderInfo","id":5,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]}},"icon":null,"color_id":-1,"is_shareable":false}],"main_chat_list_position":0,"are_tags_enabled":false}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    // `getChatFolder` response caches the full spec (keyed by folder id).
    let fetch = driver.fetch_chat_folder(5).unwrap().expect("fetch");
    assert_eq!(
        driver.fetch_chat_folder(5).unwrap(),
        None,
        "deduped in flight"
    );
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chatFolder","@extra":"{}","name":{{"@type":"chatFolderName","text":{{"@type":"formattedText","text":"Work","entities":[]}}}},"icon":null,"color_id":-1,"is_shareable":false,"pinned_chat_ids":[],"included_chat_ids":[7],"excluded_chat_ids":[],"exclude_muted":false,"exclude_read":false,"exclude_archived":false,"include_contacts":false,"include_non_contacts":true,"include_bots":false,"include_groups":false,"include_channels":false}}"#,
                        fetch.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let cached = driver.session.folder_specs.get(&5).expect("cached spec");
    assert_eq!(cached.name, "Work");
    assert_eq!(cached.included_chat_ids, vec![7]);

    // Edit sends `editChatFolder` with the full spec.
    let mut edited = cached.clone();
    edited.name = "Work stuff".to_string();
    let edit_extra = driver.edit_chat_folder(5, &edited).expect("edit");
    let json = recorder.snapshot().last().cloned().expect("sent");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editChatFolder");
    assert_eq!(v["chat_folder_id"], 5);
    assert_eq!(v["folder"]["name"]["text"]["text"], "Work stuff");
    assert_eq!(v["@extra"].as_str().unwrap(), edit_extra.0.to_string());
    driver
        .ingest(
            copy_and_parse(
                &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, edit_extra.0),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();

    // Delete sends `deleteChatFolder` with leave ids; ok drops the tab.
    let delete_extra = driver.delete_chat_folder(5, &[7]).expect("delete");
    let json = recorder.snapshot().last().cloned().expect("sent");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "deleteChatFolder");
    assert_eq!(v["chat_folder_id"], 5);
    assert_eq!(v["leave_chat_ids"], serde_json::json!([7]));
    driver
        .ingest(
            copy_and_parse(
                &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, delete_extra.0),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(driver.session.chat_folders.is_empty());
    assert!(!driver.session.folder_specs.contains_key(&5));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_reorder_chat_folders_optimistic() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatFolders","chat_folders":[{"@type":"chatFolderInfo","id":1,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"A","entities":[]}},"icon":null,"color_id":-1,"is_shareable":false},{"@type":"chatFolderInfo","id":2,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"B","entities":[]}},"icon":null,"color_id":-1,"is_shareable":false}],"main_chat_list_position":0,"are_tags_enabled":false}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    driver.reorder_chat_folders(&[2, 1]).expect("reorder");
    let json = recorder.snapshot().last().cloned().expect("sent");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "reorderChatFolders");
    assert_eq!(v["chat_folder_ids"], serde_json::json!([2, 1]));
    assert_eq!(v["main_chat_list_position"], 0);
    let ids: Vec<i32> = driver.session.chat_folders.iter().map(|f| f.id).collect();
    assert_eq!(ids, vec![2, 1], "optimistic reorder");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_toggle_chat_folder_tags() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    driver.toggle_chat_folder_tags(true).expect("toggle");
    let json = recorder.snapshot().last().cloned().expect("sent");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "toggleChatFolderTags");
    assert_eq!(v["are_tags_enabled"], true);
    assert!(driver.session.are_folder_tags_enabled);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_get_chat_lists_to_add_chat_caches() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    let first = driver
        .fetch_chat_lists_to_add_chat(ChatId(7))
        .unwrap()
        .expect("first");
    assert_eq!(
        driver.fetch_chat_lists_to_add_chat(ChatId(7)).unwrap(),
        None,
        "deduped in flight"
    );
    let json = recorder.snapshot().last().cloned().expect("sent");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getChatListsToAddChat");
    assert_eq!(v["chat_id"], 7);

    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chatLists","@extra":"{}","chat_lists":[{{"@type":"chatListMain"}},{{"@type":"chatListFolder","chat_folder_id":5}}]}}"#,
                        first.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let cached = driver
        .session
        .chat_lists_for_add
        .get(&7)
        .expect("cached lists");
    assert!(cached.contains(&crate::telegram::envelope::ChatList::Main));
    assert!(cached.contains(&crate::telegram::envelope::ChatList::Folder(5)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_add_chat_to_folder_sends_add_chat_to_list() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    let extra = driver.add_chat_to_folder(ChatId(7), 5).expect("add");
    let json = recorder.snapshot().last().cloned().expect("sent");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "addChatToList");
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["chat_list"]["@type"], "chatListFolder");
    assert_eq!(v["chat_list"]["chat_folder_id"], 5);
    assert_eq!(v["@extra"].as_str().unwrap(), extra.0.to_string());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_remove_chat_from_folder_edits_spec() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatFolders","chat_folders":[{"@type":"chatFolderInfo","id":5,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]}},"icon":null,"color_id":-1,"is_shareable":false}],"main_chat_list_position":0,"are_tags_enabled":false}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    // Chat 7 (Alice, private, user unknown → non-contact) is explicitly
    // included; the folder also matches non-contacts by filter, so
    // removal must both drop it from `included_chat_ids` and add it to
    // `excluded_chat_ids` (no `removeChatFromList` in 1.8.67).
    let fetch = driver.fetch_chat_folder(5).unwrap().expect("fetch");
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chatFolder","@extra":"{}","name":{{"@type":"chatFolderName","text":{{"@type":"formattedText","text":"Work","entities":[]}}}},"icon":null,"color_id":-1,"is_shareable":false,"pinned_chat_ids":[],"included_chat_ids":[7],"excluded_chat_ids":[],"exclude_muted":false,"exclude_read":false,"exclude_archived":false,"include_contacts":false,"include_non_contacts":true,"include_bots":false,"include_groups":false,"include_channels":false}}"#,
                        fetch.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    driver
        .remove_chat_from_folder(ChatId(7), 5)
        .expect("remove");
    assert!(
        driver.session.folder_remove_queue.is_empty(),
        "spec was cached — edit sent immediately"
    );
    let json = recorder
        .snapshot()
        .iter()
        .rev()
        .find(|j| j.contains("editChatFolder"))
        .cloned()
        .expect("editChatFolder sent");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editChatFolder");
    assert_eq!(v["chat_folder_id"], 5);
    assert_eq!(v["folder"]["included_chat_ids"], serde_json::json!([]));
    assert_eq!(
        v["folder"]["excluded_chat_ids"],
        serde_json::json!([7]),
        "filter-matched chat must be excluded or it would reappear"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_remove_chat_from_folder_waits_for_spec() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    // No cached spec: the intent queues and a `getChatFolder` goes out;
    // the edit follows once the spec arrives (via `ingest`).
    driver
        .remove_chat_from_folder(ChatId(7), 5)
        .expect("remove");
    assert_eq!(driver.session.folder_remove_queue, vec![(ChatId(7), 5)]);
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("getChatFolder")),
        "fetch sent for uncached folder"
    );
    assert!(
        !recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("editChatFolder")),
        "no edit before the spec arrives"
    );
    let fetch_extra = driver
        .session
        .requests
        .pending_extra_for_folder(RequestPurpose::GetChatFolder, 5)
        .expect("fetch in flight");
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chatFolder","@extra":"{}","name":{{"@type":"chatFolderName","text":{{"@type":"formattedText","text":"Work","entities":[]}}}},"icon":null,"color_id":-1,"is_shareable":false,"pinned_chat_ids":[],"included_chat_ids":[7],"excluded_chat_ids":[],"exclude_muted":false,"exclude_read":false,"exclude_archived":false,"include_contacts":false,"include_non_contacts":false,"include_bots":false,"include_groups":false,"include_channels":false}}"#,
                        fetch_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert!(
        driver.session.folder_remove_queue.is_empty(),
        "edit completed on spec arrival"
    );
    let json = recorder
        .snapshot()
        .iter()
        .rev()
        .find(|j| j.contains("editChatFolder"))
        .cloned()
        .expect("editChatFolder sent after spec");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["folder"]["included_chat_ids"], serde_json::json!([]));
    // No filter flags set: the chat does not match, so no exclusion.
    assert_eq!(v["folder"]["excluded_chat_ids"], serde_json::json!([]));
    let _ = std::fs::remove_dir_all(&dir);
}
