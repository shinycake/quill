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

#[test]
fn channel_admin_send_succeeds_non_admin_send_rejected() {
    // Phase 2.3: the driver gate mirrors the composer gate — an admin
    // channel sends `sendMessage` with the channel chat_id; a channel
    // without posting rights rejects like a hidden composer.
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
    // Admin channel (id 9) and plain-member channel (id 10).
    for (id, title) in [(9, "Admin news"), (10, "Member news")] {
        driver
                .ingest(
                    copy_and_parse(
                        &format!(
                            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypeSupergroup","supergroup_id":{id},"is_channel":true}},"unread_count":0}}}}"#
                        ),
                        &seq,
                        &dyn_sink,
                    )
                    .unwrap(),
                )
                .unwrap();
    }
    // getMe + getChatMember leave the viewer as an admin with the posting
    // right in channel 9.
    driver.session.my_user_id = Some(777);
    let me_extra = driver.session.request(RequestPurpose::GetMe, None);
    let admin_extra = driver
        .session
        .request(RequestPurpose::GetChatMember, Some(ChatId(9)));
    driver
        .ingest(
            copy_and_parse(
                &format!(r#"{{"@type":"user","@extra":"{}","id":777}}"#, me_extra.0),
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
                        r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{{"@type":"chatAdministratorRights","can_post_messages":true}}}}}}"#,
                        admin_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert!(driver.session.chats.get(&9).unwrap().can_post());

    let snap = crate::composer::ComposerSnapshot::capture(
        ChatId(9),
        driver.session.view_generation,
        "CANARYADMINpost",
    );
    let send_extra = driver.send_text_snapshot(&snap).unwrap();
    let sent = recorder.snapshot();
    let send_json = sent.last().unwrap();
    assert!(send_json.contains("sendMessage"));
    assert!(send_json.contains("\"chat_id\":9"));
    assert!(send_json.contains("CANARYADMINpost"));
    assert!(send_json.contains(&format!("\"@extra\":\"{}\"", send_extra.0)));

    // Channel 10: membership unknown → composer hidden, send rejected.
    assert!(!driver.session.chats.get(&10).unwrap().can_post());
    let member_snap = crate::composer::ComposerSnapshot::capture(
        ChatId(10),
        driver.session.view_generation,
        "nope",
    );
    assert_eq!(
        driver.send_text_snapshot(&member_snap),
        Err(ConnectSendError::InvalidRequest)
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bot_info_fetched_once_on_chat_open() {
    // Phase 3.1: opening a bot chat lazily sends `getUserFullInfo` once;
    // the `userFullInfo` response populates the cache and suppresses
    // refetches. Non-bot chats never trigger the fetch.
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
    // Bot private chat (id 21) and a regular private chat (id 22).
    for json in [
        r#"{"@type":"updateUser","user":{"id":21,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":22,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":22},"unread_count":0}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    // The bot chat rides the ordinary private-chat path: no gate, and
    // the composer is shown.
    let bot_chat = driver.session.chats.get(&21).unwrap();
    assert!(bot_chat.supported());
    assert!(bot_chat.kind.gate_reason().is_none());
    assert!(bot_chat.can_post());

    driver.select_chat(ChatId(21)).unwrap();
    let info_fetches = || {
        recorder
            .snapshot()
            .into_iter()
            .filter(|j| j.contains("\"@type\":\"getUserFullInfo\""))
            .collect::<Vec<_>>()
    };
    let first = info_fetches();
    assert_eq!(first.len(), 1);
    assert!(first[0].contains("\"user_id\":21"));
    let extra = driver
        .session
        .requests
        .pending_extra_for(RequestPurpose::GetUserFullInfo, Some(ChatId(21)));

    // Re-selecting while the fetch is in flight sends nothing new.
    driver.select_chat(ChatId(21)).unwrap();
    assert_eq!(info_fetches().len(), 1);

    // The response populates the cache; further opens stay quiet.
    let extra = extra.expect("getUserFullInfo in flight");
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"s","description":"CANARY_bot_desc","commands":[{{"@type":"botCommand","command":"start","description":"Start","is_ephemeral":false}}]}}}}"#,
                        extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let info = driver
        .session
        .bot_info_for_chat(ChatId(21))
        .expect("bot info cached");
    assert_eq!(info.description, "CANARY_bot_desc");
    assert_eq!(info.commands.len(), 1);
    assert_eq!(info.commands[0].command, "start");
    driver.select_chat(ChatId(21)).unwrap();
    assert_eq!(info_fetches().len(), 1);

    // A regular private chat never triggers the fetch.
    driver.select_chat(ChatId(22)).unwrap();
    assert_eq!(info_fetches().len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn contacts_fetched_once_for_tab() {
    // Phase 6: `fetch_contacts` sends `getContacts` once; a second
    // call while the fetch is in flight or after the `users` response
    // lands sends nothing new.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let extra = driver.fetch_contacts().unwrap().expect("getContacts sent");
    let sent = recorder.snapshot();
    let get = sent
        .iter()
        .find(|j| j.contains(r#""@type":"getContacts""#))
        .expect("getContacts in outbox");
    assert!(get.contains(&format!(r#""@extra":"{}""#, extra.0)));
    // In flight → no-op.
    assert_eq!(driver.fetch_contacts().unwrap(), None);
    assert_eq!(recorder.snapshot().len(), sent.len());
    // The `users` response settles the list; further calls stay quiet.
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"users","@extra":"{}","total_count":1,"user_ids":[31]}}"#,
                    extra.0,
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(driver.session.contacts.as_deref(), Some([31].as_slice()));
    assert_eq!(driver.fetch_contacts().unwrap(), None);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn user_full_info_fetched_once_per_user() {
    // Phase 6: `fetch_user_full_info` sends `getUserFullInfo` once per
    // user; the `userFullInfo` response (correlated by
    // `PendingRequest::user_id`, not by chat) caches the bio and
    // suppresses refetches.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let extra = driver
        .fetch_user_full_info(31)
        .unwrap()
        .expect("getUserFullInfo sent");
    let sent = recorder.snapshot();
    let info_json = sent
        .iter()
        .find(|j| j.contains(r#""@type":"getUserFullInfo""#))
        .expect("getUserFullInfo in outbox");
    assert!(info_json.contains(r#""user_id":31"#));
    // In flight → no-op.
    assert_eq!(driver.fetch_user_full_info(31).unwrap(), None);
    assert_eq!(recorder.snapshot().len(), sent.len());
    // The response caches the bio; further fetches stay quiet.
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"userFullInfo","@extra":"{}","bio":{{"@type":"formattedText","text":"CANARY_bio","entities":[]}},"bot_info":null}}"#,
                        extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(
        driver.session.user_full_info(31).map(|i| i.bio.as_str()),
        Some("CANARY_bio")
    );
    assert_eq!(driver.fetch_user_full_info(31).unwrap(), None);
    // A different user still fetches.
    assert!(driver.fetch_user_full_info(32).unwrap().is_some());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn supergroup_full_info_fetched_once() {
    // Phase 6: `fetch_supergroup_full_info` sends `getSupergroupFullInfo`
    // once per supergroup; the id-less `supergroupFullInfo` response
    // (correlated by `PendingRequest::supergroup_id`) caches the
    // description + member count.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let extra = driver
        .fetch_supergroup_full_info(77)
        .unwrap()
        .expect("getSupergroupFullInfo sent");
    let sent = recorder.snapshot();
    let info_json = sent
        .iter()
        .find(|j| j.contains(r#""@type":"getSupergroupFullInfo""#))
        .expect("getSupergroupFullInfo in outbox");
    assert!(info_json.contains(r#""supergroup_id":77"#));
    assert_eq!(driver.fetch_supergroup_full_info(77).unwrap(), None);
    assert_eq!(recorder.snapshot().len(), sent.len());
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"supergroupFullInfo","@extra":"{}","description":"CANARY_desc","member_count":4321}}"#,
                        extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let info = driver.session.supergroup_full_info(77).expect("cached");
    assert_eq!(info.description, "CANARY_desc");
    assert_eq!(info.member_count, 4321);
    assert_eq!(driver.fetch_supergroup_full_info(77).unwrap(), None);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn add_contact_sends_imported_contact() {
    // Phase 6: `add_contact` sends `addContact` with the
    // `importedContact` shape; the `ok` answer invalidates the contacts
    // list so the tab refetches it.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    driver.session.contacts = Some(Vec::new());
    let extra = driver
        .add_contact(31, "+15550131", "Ada", "Lovelace")
        .unwrap()
        .expect("addContact sent");
    let sent = recorder.snapshot();
    let add = sent
        .iter()
        .find(|j| j.contains(r#""@type":"addContact""#))
        .expect("addContact in outbox");
    assert!(add.contains(r#""user_id":31"#));
    assert!(add.contains(r#""@type":"importedContact""#));
    assert!(add.contains(r#""phone_number":"+15550131""#));
    assert!(add.contains(r#""first_name":"Ada""#));
    assert!(add.contains(r#""last_name":"Lovelace""#));
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
    assert!(driver.session.contacts.is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a6_remove_contact_sends_and_invalidates() {
    // Slice A6: `remove_contact` sends `removeContacts([user_id])`
    // (schema 1.8.67, line 14528); the `ok` answer invalidates the
    // contacts list and records the notice — never optimistic.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    driver.session.contacts = Some(vec![31]);
    let extra = driver
        .remove_contact(31)
        .unwrap()
        .expect("removeContacts sent");
    let sent = recorder.snapshot();
    let remove = sent
        .iter()
        .find(|j| j.contains(r#""@type":"removeContacts""#))
        .expect("removeContacts in outbox");
    assert!(remove.contains(r#""user_ids":[31]"#));
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
    assert!(driver.session.contacts.is_none());
    assert_eq!(
        driver.session.contacts_notice.as_deref(),
        Some("Contact deleted.")
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a6_delete_synced_contacts_clears_then_removes() {
    // Slice A6: `delete_synced_contacts` sends `clearImportedContacts`
    // first (the server-side wipe, schema 1.8.67 line 14539), then
    // `removeContacts` for the cached ids (TGX `deleteContacts`).
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    driver.session.contacts = Some(vec![31, 32]);
    let sent_count = driver.delete_synced_contacts().unwrap();
    assert_eq!(sent_count, 2);
    let sent = recorder.snapshot();
    let clear_pos = sent
        .iter()
        .position(|j| j.contains(r#""@type":"clearImportedContacts""#))
        .expect("clearImportedContacts in outbox");
    let remove_pos = sent
        .iter()
        .position(|j| j.contains(r#""@type":"removeContacts""#))
        .expect("removeContacts in outbox");
    assert!(clear_pos < remove_pos);
    assert!(sent[remove_pos].contains(r#""user_ids":[31,32]"#));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bot_commands_fetched_once_on_chat_open() {
    // Phase 3.3: opening a bot chat lazily sends `getCommands` once
    // (null scope selects the default scope, schema 1.8.67 line
    // 14953). The `botCommands` response populates the cache and
    // merges below the `botInfo` commands in `command_menu_items`;
    // an `error` answer is recorded as an empty set so the fetch is
    // never retried. Non-bot chats never trigger the fetch.
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
    // Bot private chat (id 21) and a regular private chat (id 22).
    for json in [
        r#"{"@type":"updateUser","user":{"id":21,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":22,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":22},"unread_count":0}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }

    driver.select_chat(ChatId(21)).unwrap();
    let cmd_fetches = || {
        recorder
            .snapshot()
            .into_iter()
            .filter(|j| j.contains("\"@type\":\"getCommands\""))
            .collect::<Vec<_>>()
    };
    let first = cmd_fetches();
    assert_eq!(first.len(), 1);
    assert!(first[0].contains("\"scope\":null"));
    assert!(first[0].contains("\"language_code\":\"\""));
    let extra = driver
        .session
        .requests
        .pending_extra_for(RequestPurpose::GetCommands, Some(ChatId(21)))
        .expect("getCommands in flight");

    // Re-selecting while the fetch is in flight sends nothing new.
    driver.select_chat(ChatId(21)).unwrap();
    assert_eq!(cmd_fetches().len(), 1);

    // The `botCommands` response lands in the cache as global rows.
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"botCommands","@extra":"{}","bot_user_id":21,"commands":[{{"@type":"botCommand","command":"settings","description":"CANARY_global","is_ephemeral":false}}]}}"#,
                        extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let items = driver.session.command_menu_items(ChatId(21));
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].command, "settings");
    assert_eq!(items[0].description, "CANARY_global");
    assert!(items[0].global);
    driver.select_chat(ChatId(21)).unwrap();
    assert_eq!(cmd_fetches().len(), 1);

    // `botInfo` commands merge first; duplicates keep the
    // bot-specific description and are not repeated.
    let full_extra = driver
        .session
        .request(RequestPurpose::GetUserFullInfo, Some(ChatId(21)));
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"","description":"","commands":[{{"@type":"botCommand","command":"start","description":"Start","is_ephemeral":false}},{{"@type":"botCommand","command":"settings","description":"Specific settings","is_ephemeral":false}}]}}}}"#,
                        full_extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let items = driver.session.command_menu_items(ChatId(21));
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].command, "start");
    assert!(!items[0].global);
    assert_eq!(items[1].command, "settings");
    assert_eq!(items[1].description, "Specific settings");
    assert!(!items[1].global);

    // A regular private chat never triggers the fetch.
    driver.select_chat(ChatId(22)).unwrap();
    assert_eq!(cmd_fetches().len(), 1);
    assert!(driver.session.command_menu_items(ChatId(22)).is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bot_commands_error_absorbed_without_retry() {
    // Phase 3.3: an `error` answer to `getCommands` (user sessions —
    // the schema annotates the method "for bots only") is recorded as
    // an empty command set, so opening the chat again does not
    // refetch; the menu falls back to the `botInfo` commands.
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
    for json in [
        r#"{"@type":"updateUser","user":{"id":21,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    // Seed `botInfo` so the fallback menu has rows after the error.
    let full_extra = driver
        .session
        .request(RequestPurpose::GetUserFullInfo, Some(ChatId(21)));
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"","description":"","commands":[{{"@type":"botCommand","command":"start","description":"Start","is_ephemeral":false}}]}}}}"#,
                        full_extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    driver.select_chat(ChatId(21)).unwrap();
    let cmd_fetches = || {
        recorder
            .snapshot()
            .into_iter()
            .filter(|j| j.contains("\"@type\":\"getCommands\""))
            .count()
    };
    assert_eq!(cmd_fetches(), 1);
    let extra = driver
        .session
        .requests
        .pending_extra_for(RequestPurpose::GetCommands, Some(ChatId(21)))
        .expect("getCommands in flight");
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"error","@extra":"{}","code":400,"message":"CANARY_bots_only"}}"#,
                    extra.0,
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();

    // The error records an empty set; re-opening the chat refetches
    // nothing, and the menu shows the `botInfo` commands only.
    driver.select_chat(ChatId(21)).unwrap();
    assert_eq!(cmd_fetches(), 1);
    let items = driver.session.command_menu_items(ChatId(21));
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].command, "start");
    assert!(!items[0].global);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn callback_query_sent_for_button_press() {
    // Phase 3.2: pressing a callback button sends `getCallbackQueryAnswer`
    // (schema 1.8.67 line 13138) with the button's payload bytes
    // (base64 in JSON). Pending messages and unknown chats are refused.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    for json in [
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        r#"{"@type":"updateUser","user":{"id":21,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
        r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Vote","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeCallback","data":"AQID"}}]],"force_reply":false},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Pick","entities":[]}}}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    let extra = driver
        .send_callback_query(ChatId(21), MessageId(301), &[1, 2, 3])
        .expect("callback query sends");
    let sent = recorder.snapshot();
    let query = sent
        .iter()
        .find(|j| j.contains(r#""@type":"getCallbackQueryAnswer""#))
        .expect("getCallbackQueryAnswer recorded");
    let v: serde_json::Value = serde_json::from_str(query).unwrap();
    assert_eq!(v["chat_id"], 21);
    assert_eq!(v["message_id"], 301);
    assert_eq!(v["payload"]["@type"], "callbackQueryPayloadData");
    assert_eq!(v["payload"]["data"], "AQID");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert!(
        driver
            .session
            .requests
            .pending_extra_for(RequestPurpose::GetCallbackQueryAnswer, Some(ChatId(21)))
            .is_some()
    );
    // Pending (unsent, negative id) messages cannot be answered.
    assert!(
        driver
            .send_callback_query(ChatId(21), MessageId(-1), &[1])
            .is_err()
    );
    // Unknown chats are refused.
    assert!(
        driver
            .send_callback_query(ChatId(99), MessageId(301), &[1])
            .is_err()
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn select_chat_closes_previous_and_does_not_mark_unread_locally() {
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
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":3,"last_read_inbox_message_id":1}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":1}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver.select_chat(ChatId(7)).unwrap();
    assert_eq!(driver.session.chats.get(&7).unwrap().unread_count, 3);
    driver.select_chat(ChatId(8)).unwrap();
    let sent = recorder.snapshot();
    assert!(
        sent.iter()
            .any(|j| j.contains("\"@type\":\"closeChat\"") && j.contains("\"chat_id\":7"))
    );
    assert!(
        sent.iter()
            .any(|j| j.contains("\"@type\":\"openChat\"") && j.contains("\"chat_id\":8"))
    );
    // Unread is TDLib-authoritative; opening must not zero it locally.
    assert_eq!(driver.session.chats.get(&7).unwrap().unread_count, 3);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatReadInbox","chat_id":7,"last_read_inbox_message_id":9,"unread_count":0}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(driver.session.chats.get(&7).unwrap().unread_count, 0);
    assert!(!sink.rendered().contains("Alice"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_open_chat_is_closed_when_leaving_it() {
    // `openChat` / `closeChat` must pair (schema 1.8.67: "Informs TDLib
    // that the chat is opened/closed by the user"); Telegram X sends
    // `CloseChat` for every chat it opened (`Tdlib.openChat` /
    // `Tdlib.closeChatImpl` track `openedChats`). A chat whose type is not
    // known yet is still opened, so it must be closed.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    for json in [
        r#"{"@type":"updateChatPosition","chat_id":9,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"5","is_pinned":false}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":0}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    driver.select_chat(ChatId(9)).unwrap();
    driver.select_chat(ChatId(8)).unwrap();
    let opened_9 = recorder
        .snapshot()
        .iter()
        .filter(|j| j.contains("\"@type\":\"openChat\"") && j.contains("\"chat_id\":9"))
        .count();
    let closed_9 = recorder
        .snapshot()
        .iter()
        .filter(|j| j.contains("\"@type\":\"closeChat\"") && j.contains("\"chat_id\":9"))
        .count();
    assert_eq!(opened_9, 1);
    assert_eq!(closed_9, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn archive_list_is_loaded_after_the_main_list() {
    // TDLib reports a list's chats only once the list is loaded: "The
    // loaded chats and their positions in the chat list will be sent
    // through updates" (`loadChats`, schema 1.8.67, line 11595). The
    // archive was never loaded, so it showed only chats that happened to
    // get a position. Telegram X loads every list it shows through
    // `loadChats` (`TdlibChatList.loadMore`).
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let load_chats = |recorder: &RecordingSender| -> Vec<Value> {
        recorder
            .snapshot()
            .iter()
            .filter_map(|j| serde_json::from_str::<Value>(j).ok())
            .filter(|v| v["@type"] == "loadChats")
            .collect()
    };
    let answer = |driver: &mut ConnectDriver<Arc<RecordingSender>>, extra: &Value, ok: bool| {
        let extra = extra.as_str().unwrap();
        let json = if ok {
            format!(r#"{{"@type":"ok","@extra":"{extra}"}}"#)
        } else {
            format!(r#"{{"@type":"error","@extra":"{extra}","code":404,"message":"Not Found"}}"#)
        };
        driver
            .ingest(copy_and_parse(&json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };
    let sent = load_chats(&recorder);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["chat_list"]["@type"], "chatListMain");
    // Main list exhausted → the archive starts paging.
    answer(&mut driver, &sent[0]["@extra"], false);
    let sent = load_chats(&recorder);
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[1]["chat_list"]["@type"], "chatListArchive");
    assert_eq!(sent[1]["limit"], MAIN_CHAT_LOAD_LIMIT);
    // ok → next archive page; 404 → done.
    answer(&mut driver, &sent[1]["@extra"], true);
    let sent = load_chats(&recorder);
    assert_eq!(sent.len(), 3);
    assert_eq!(sent[2]["chat_list"]["@type"], "chatListArchive");
    answer(&mut driver, &sent[2]["@extra"], false);
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateChatTitle","chat_id":1,"title":"x"}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(load_chats(&recorder).len(), 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_dismisses_and_shares_through_the_action_bar() {
    // Batch 8: close sends `removeChatActionBar` and drops the bar; Share
    // my phone number sends `sharePhoneNumber` for the peer user.
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    for json in [
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":501,"title":"Stranger","type":{"@type":"chatTypePrivate","user_id":501},"action_bar":{"@type":"chatActionBarSharePhoneNumber"}}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    assert!(driver.session.chat_action_bar(ChatId(501)).is_some());
    driver.share_phone_number(ChatId(501), 501).unwrap();
    assert!(driver.session.chat_action_bar(ChatId(501)).is_none());
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("\"sharePhoneNumber\"") && j.contains("\"user_id\":501"))
    );

    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateChatActionBar","chat_id":501,"action_bar":{"@type":"chatActionBarAddContact"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    driver.dismiss_chat_action_bar(ChatId(501)).unwrap();
    assert!(driver.session.chat_action_bar(ChatId(501)).is_none());
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("\"removeChatActionBar\"") && j.contains("\"chat_id\":501"))
    );
    // Nothing to dismiss now: no second request.
    assert_eq!(driver.dismiss_chat_action_bar(ChatId(501)), Ok(None));
}

#[test]
fn driver_folder_share_requests_and_optimistic_delete() {
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
        driver.fetch_recommended_chat_folders(),
        Err(ConnectSendError::InvalidRequest)
    );
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    driver.fetch_folder_share(5).expect("share fetch");
    let sent = recorder.snapshot();
    assert!(sent.iter().any(|j| j.contains("getChatFolderInviteLinks")));
    assert!(
        sent.iter()
            .any(|j| j.contains("getChatsForChatFolderInviteLink"))
    );
    // In-flight requests are not duplicated.
    driver.fetch_folder_share(5).expect("deduped");
    assert_eq!(
        recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("getChatFolderInviteLinks"))
            .count(),
        1
    );

    driver.session.folder_invite_links.insert(
        5,
        vec![crate::telegram::envelope::ChatFolderInviteLink {
            invite_link: "https://t.me/addlist/a".into(),
            name: String::new(),
            chat_ids: vec![7],
        }],
    );
    driver
        .delete_folder_invite_link(5, "https://t.me/addlist/a")
        .expect("delete");
    assert!(driver.session.folder_invite_links[&5].is_empty());

    driver
        .check_folder_invite_link("https://t.me/addlist/b")
        .expect("check");
    assert_eq!(
        driver.session.folder_invite_link.as_deref(),
        Some("https://t.me/addlist/b")
    );
    driver
        .add_folder_by_invite_link("https://t.me/addlist/b", &[7, 8])
        .expect("add");
    let last = recorder.snapshot().last().cloned().unwrap();
    let v: Value = serde_json::from_str(&last).unwrap();
    assert_eq!(v["@type"], "addChatFolderByInviteLink");
    assert_eq!(v["chat_ids"], serde_json::json!([7, 8]));
    let _ = std::fs::remove_dir_all(&dir);
}
