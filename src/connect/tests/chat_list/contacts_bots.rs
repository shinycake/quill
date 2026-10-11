//! Driver tests: channel posting rights, bot info and commands, contacts and full-info fetches.
use super::*;

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
    assert_eq!(
        driver.session.users_state.contacts.as_deref(),
        Some([31].as_slice())
    );
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
    driver.session.users_state.contacts = Some(Vec::new());
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
    assert!(driver.session.users_state.contacts.is_none());

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
    driver.session.users_state.contacts = Some(vec![31]);
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
    assert!(driver.session.users_state.contacts.is_none());
    assert_eq!(
        driver.session.users_state.contacts_notice.as_deref(),
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
    driver.session.users_state.contacts = Some(vec![31, 32]);
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
