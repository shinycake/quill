//! Driver tests: data-saving calls, chat loading after Ready, and sending text.
use super::*;

/// Slice calls-less-data: the "Use less data for calls" toggle pushes
/// `setAutoDownloadSettings` with `use_less_data_for_calls` for all three
/// networks, and refuses while local prefs are unseeded (so the all-off
/// defaults never silently disable the user's auto-downloads).
#[test]
fn less_data_for_calls_pushes_all_networks_and_refuses_unseeded() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    let ready = copy_and_parse(
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        &seq,
        &dyn_sink,
    )
    .unwrap();
    driver.ingest(ready).unwrap();

    // Unseeded: refused, no setAutoDownloadSettings sent.
    assert!(driver.set_less_data_for_calls(true).is_err());
    assert!(
        recorder
            .snapshot()
            .iter()
            .all(|json| !json.contains(r#""@type":"setAutoDownloadSettings""#))
    );

    driver.session.data_storage.seeded = true;
    driver.set_less_data_for_calls(true).unwrap();
    let sent: Vec<Value> = recorder
        .snapshot()
        .iter()
        .map(|json| serde_json::from_str(json).expect("sent json parses"))
        .collect();
    let less: Vec<&Value> = sent
        .iter()
        .filter(|v| v["@type"] == "setAutoDownloadSettings")
        .collect();
    assert_eq!(less.len(), 3);
    let networks: Vec<&str> = less
        .iter()
        .map(|v| v["type"]["@type"].as_str().expect("network type"))
        .collect();
    assert_eq!(
        networks,
        vec![
            "networkTypeMobile",
            "networkTypeMobileRoaming",
            "networkTypeWiFi"
        ]
    );
    for req in &less {
        assert_eq!(req["settings"]["use_less_data_for_calls"], true);
    }

    drop(driver);
    drop(recorder);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_loads_chats_after_ready_then_send_text() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);

    assert_eq!(
        driver.select_chat(ChatId(1)),
        Err(ConnectSendError::InvalidRequest)
    );

    let seq = AtomicU64::new(0);
    let ready = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(ready).unwrap();
    assert!(matches!(driver.session.auth, AuthorizationState::Ready));

    let sent = recorder.snapshot();
    // Phase 9.1: `loadActiveStories(storyListMain)` follows `loadChats`
    // after Ready (feeds the story tray). Parity slice: the notification
    // slice then fetches the saved-sound list and the three scope
    // defaults (`getSavedNotificationSounds`,
    // `getScopeNotificationSettings` × 3).
    let types: Vec<&str> = sent
        .iter()
        .map(|json| {
            if json.contains(r#""@type":"loadChats""#) {
                "loadChats"
            } else if json.contains(r#""@type":"loadActiveStories""#) {
                "loadActiveStories"
            } else if json.contains(r#""@type":"getSavedNotificationSounds""#) {
                "getSavedNotificationSounds"
            } else if json.contains(r#""@type":"getScopeNotificationSettings""#) {
                "getScopeNotificationSettings"
            } else {
                "other"
            }
        })
        .collect();
    assert_eq!(
        types,
        vec![
            "loadChats",
            "loadActiveStories",
            "getSavedNotificationSounds",
            "getScopeNotificationSettings",
            "getScopeNotificationSettings",
            "getScopeNotificationSettings",
        ]
    );
    let load = &sent[0];
    assert!(load.contains("chatListMain"));
    assert!(load.contains(&format!("\"limit\":{MAIN_CHAT_LOAD_LIMIT}")));
    let load_extra = driver
        .session
        .requests
        .has_purpose(RequestPurpose::LoadChats);
    assert!(load_extra);

    let new_chat = copy_and_parse(
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(new_chat).unwrap();
    let position = copy_and_parse(
            r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"12","is_pinned":false}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(position).unwrap();
    assert_eq!(driver.session.ordered_chats()[0].id.0, 7);

    // In-flight loadChats: ingest of unrelated updates must not send another page.
    let loads_before = recorder
        .snapshot()
        .iter()
        .filter(|j| j.contains("loadChats"))
        .count();
    assert_eq!(loads_before, 1);

    assert!(
        driver
            .session
            .requests
            .has_purpose(RequestPurpose::LoadChats)
    );
    let load_ok = copy_and_parse(r#"{"@type":"ok","@extra":"1"}"#, &seq, &dyn_sink).unwrap();
    driver.ingest(load_ok).unwrap();
    // ok on loadChats means more may exist — one continuation page, not a per-tick loop.
    let loads_after_ok = recorder
        .snapshot()
        .iter()
        .filter(|j| j.contains("loadChats"))
        .count();
    assert_eq!(loads_after_ok, 2);

    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateUpdating"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("loadChats"))
            .count(),
        2,
        "unrelated ingest must not re-page loadChats"
    );

    let last_load = recorder
        .snapshot()
        .into_iter()
        .rev()
        .find(|j| j.contains("loadChats"))
        .unwrap();
    let v: Value = serde_json::from_str(&last_load).unwrap();
    let extra = v["@extra"].as_str().unwrap();
    let err404 = copy_and_parse(
        &format!(
            r#"{{"@type":"error","code":404,"message":"Not Found","@extra":"{id}"}}"#,
            id = extra
        ),
        &seq,
        &dyn_sink,
    )
    .unwrap();
    driver.ingest(err404).unwrap();
    assert!(driver.session.chats_exhausted);
    // The main list's 404 starts archive paging (one page in flight);
    // the main list itself pages no further.
    let main_loads = |recorder: &RecordingSender| {
        recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("loadChats") && j.contains("chatListMain"))
            .count()
    };
    assert_eq!(main_loads(&recorder), 2);
    let loads_done = recorder
        .snapshot()
        .iter()
        .filter(|j| j.contains("loadChats"))
        .count();
    assert_eq!(loads_done, 3);
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("loadChats"))
            .count(),
        loads_done
    );
    assert!(!sink.rendered().contains("Not Found"));

    let history_extra = driver.select_chat(ChatId(7)).unwrap().expect("history");
    let sent = recorder.snapshot();
    assert!(
        sent.iter()
            .any(|j| j.contains("\"@type\":\"openChat\"") && j.contains("\"chat_id\":7")),
        "select_chat must send openChat"
    );
    let history_json = sent.last().unwrap();
    assert!(history_json.contains("getChatHistory"));
    assert!(history_json.contains("\"chat_id\":7"));
    assert!(history_json.contains(&format!("\"@extra\":\"{}\"", history_extra.0)));

    let messages = copy_and_parse(
            &format!(
                r#"{{"@type":"messages","@extra":"{}","messages":[{{"id":11,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}]}}"#,
                history_extra.0
            ),
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(messages).unwrap();
    assert!(
        driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .contains_key(&11)
    );
    let view_json = recorder
        .snapshot()
        .into_iter()
        .rev()
        .find(|j| j.contains("viewMessages"))
        .expect("viewMessages after history");
    let view: Value = serde_json::from_str(&view_json).unwrap();
    assert_eq!(view["@type"], "viewMessages");
    assert_eq!(view["chat_id"], 7);
    assert_eq!(view["message_ids"], serde_json::json!([11]));
    assert_eq!(view["source"]["@type"], "messageSourceChatHistory");
    assert_eq!(view["force_read"], true);
    assert!(!view_json.contains("hi"));

    let snap = crate::composer::ComposerSnapshot::capture(
        ChatId(7),
        driver.session.view_generation,
        "CANARYSENDping",
    );
    let send_extra = driver.send_text_snapshot(&snap).unwrap();
    let sent = recorder.snapshot();
    let send_json = sent.last().unwrap();
    assert!(send_json.contains("sendMessage"));
    assert!(send_json.contains("\"topic_id\":null"));
    assert!(send_json.contains("CANARYSENDping"));
    assert!(send_json.contains(&format!("\"@extra\":\"{}\"", send_extra.0)));
    assert!(!sink.rendered().contains("CANARYSEND"));

    let channel_no_post = copy_and_parse(
            r#"{"@type":"updateNewChat","chat":{"id":8,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":8,"is_channel":true},"unread_count":0}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(channel_no_post).unwrap();
    let channel_snap = crate::composer::ComposerSnapshot::capture(
        ChatId(8),
        driver.session.view_generation,
        "nope",
    );
    assert_eq!(
        driver.send_text_snapshot(&channel_snap),
        Err(ConnectSendError::InvalidRequest)
    );
    assert!(!sink.rendered().contains("CANARYSEND"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_send_text_snapshot_in_topic_addresses_message_topic_forum() {
    // Parity slice 4: a send from a topic view carries
    // `topic_id = messageTopicForum{forum_topic_id}` (schema 1.8.67,
    // lines 12200 / 3004); a closed topic rejects the send.
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
        r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Demo forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
        r#"{"@type":"updateChatPosition","chat_id":16,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"25","is_pinned":false}}"#,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":true}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    // Inject the topic list through the reducer; topic 3 is closed.
    let extra = driver
        .session
        .request(RequestPurpose::GetForumTopics, Some(ChatId(16)));
    let topics = [(2, "General", false), (3, "Random", true)]
            .iter()
            .map(|(id, name, closed)| {
                format!(
                    "{{\"info\":{{\"@type\":\"forumTopicInfo\",\"chat_id\":16,\"forum_topic_id\":{id},\"name\":\"{name}\",\"is_general\":false,\"is_closed\":{closed}}},\"order\":\"{id}\",\"is_pinned\":false,\"unread_count\":0}}"
                )
            })
            .collect::<Vec<_>>()
            .join(",");
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        "{{\"@type\":\"forumTopics\",\"@extra\":\"{}\",\"total_count\":2,\"topics\":[{topics}],\"next_offset_date\":0,\"next_offset_message_id\":0,\"next_offset_forum_topic_id\":0}}",
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    driver.session.open_chat(ChatId(16));
    driver.session.select_topic(ChatId(16), 2);
    let snap = crate::composer::ComposerSnapshot::capture(
        ChatId(16),
        driver.session.view_generation,
        "CANARYTOPICping",
    );
    let send_extra = driver.send_text_snapshot(&snap).unwrap();
    let sent = recorder.snapshot();
    let send_json = sent.last().unwrap();
    let v: Value = serde_json::from_str(send_json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["chat_id"], 16);
    assert_eq!(v["topic_id"]["@type"], "messageTopicForum");
    assert_eq!(v["topic_id"]["forum_topic_id"], 2);
    assert!(send_json.contains("CANARYTOPICping"));
    assert!(send_json.contains(&format!("\"@extra\":\"{}\"", send_extra.0)));

    // A closed topic rejects the send (the composer is hidden there;
    // this guards a stale-snapshot race).
    driver.session.select_topic(ChatId(16), 3);
    let closed_snap = crate::composer::ComposerSnapshot::capture(
        ChatId(16),
        driver.session.view_generation,
        "nope",
    );
    assert_eq!(
        driver.send_text_snapshot(&closed_snap),
        Err(ConnectSendError::InvalidRequest)
    );

    let _ = std::fs::remove_dir_all(&dir);
}
