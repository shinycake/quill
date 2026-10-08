//! Connect-driver tests: supergroup settings, forums, boosts, welcome messages.
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

/// Slice G2: `toggle_sign_messages` gates on `can_change_info` in a
/// channel, sends the right JSON, applies optimistically, and rolls
/// back on a TDLib error.
#[test]
fn driver_toggle_sign_messages_sends_and_rolls_back() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let ingest_json = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };
    ingest_json(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"c","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
    );
    // A plain member is gated out.
    ingest_json(
        &mut driver,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"status":{"@type":"chatMemberStatusMember"}}}"#,
    );
    assert_eq!(
        driver.toggle_sign_messages(ChatId(13), true, true).unwrap(),
        None
    );
    // An admin with `can_change_info` passes the gate.
    ingest_json(
        &mut driver,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"status":{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_change_info":true}}}}"#,
    );
    let extra = driver
        .toggle_sign_messages(ChatId(13), true, true)
        .unwrap()
        .expect("toggle sent");
    let sent = recorder
        .snapshot()
        .into_iter()
        .find(|j| j.contains("toggleSupergroupSignMessages"))
        .expect("toggle JSON sent");
    let value: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(value["@type"], "toggleSupergroupSignMessages");
    assert_eq!(value["supergroup_id"], 13);
    assert_eq!(value["sign_messages"], true);
    assert_eq!(value["show_message_sender"], true);
    // Optimistic state.
    assert_eq!(
        driver.session.supergroup_sign_messages.get(&13),
        Some(&true)
    );
    // In flight → no-op.
    assert_eq!(
        driver.toggle_sign_messages(ChatId(13), true, true).unwrap(),
        None
    );
    // A TDLib error rolls the flags back.
    ingest_json(
        &mut driver,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_ADMIN_REQUIRED"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        driver.session.supergroup_sign_messages.get(&13),
        Some(&false)
    );
    assert_eq!(
        driver.session.supergroup_show_message_sender.get(&13),
        Some(&false)
    );
    // A non-channel supergroup is not eligible at all.
    ingest_json(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":14,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false},"unread_count":0}}"#,
    );
    assert_eq!(
        driver.toggle_sign_messages(ChatId(14), true, true).unwrap(),
        None
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice G2: `toggle_aggressive_anti_spam` is gated on
/// `supergroupFullInfo.can_toggle_aggressive_anti_spam`.
#[test]
fn driver_toggle_anti_spam_gated_on_full_info_capability() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let ingest_json = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };
    ingest_json(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":false},"unread_count":0}}"#,
    );
    // No full info → the toggle is not offered (quiet no-op).
    assert_eq!(
        driver
            .toggle_aggressive_anti_spam(ChatId(13), true)
            .unwrap(),
        None
    );
    ingest_json(
        &mut driver,
        r#"{"@type":"updateSupergroupFullInfo","supergroup_id":13,"supergroup_full_info":{"@type":"supergroupFullInfo","has_aggressive_anti_spam_enabled":false,"can_toggle_aggressive_anti_spam":true}}"#,
    );
    let extra = driver
        .toggle_aggressive_anti_spam(ChatId(13), true)
        .unwrap()
        .expect("toggle sent");
    let sent = recorder
        .snapshot()
        .into_iter()
        .find(|j| j.contains("toggleSupergroupHasAggressiveAntiSpamEnabled"))
        .expect("anti-spam JSON sent");
    let value: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(value["supergroup_id"], 13);
    assert_eq!(value["has_aggressive_anti_spam_enabled"], true);
    assert_eq!(
        driver.session.supergroup_anti_spam_enabled.get(&13),
        Some(&true)
    );
    ingest_json(
        &mut driver,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_ADMIN_REQUIRED"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        driver.session.supergroup_anti_spam_enabled.get(&13),
        Some(&false)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice G2: forum-topic mutations gate on `can_manage_topics`,
/// reject empty names, and send the right constructors.
#[test]
fn driver_forum_topic_mutations_send_and_gate() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let ingest_json = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };
    ingest_json(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":false},"unread_count":0}}"#,
    );
    ingest_json(
        &mut driver,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"status":{"@type":"chatMemberStatusMember"}}}"#,
    );
    // A plain member is gated out.
    assert_eq!(driver.create_forum_topic(ChatId(13), "news").unwrap(), None);
    // Empty names are rejected.
    assert!(driver.create_forum_topic(ChatId(13), "  ").is_err());
    // An admin with `can_manage_topics` passes.
    ingest_json(
        &mut driver,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"status":{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_manage_topics":true}}}}"#,
    );
    let extra = driver
        .create_forum_topic(ChatId(13), "news")
        .unwrap()
        .expect("create sent");
    let sent: Vec<Value> = recorder
        .snapshot()
        .into_iter()
        .filter(|j| j.contains("createForumTopic"))
        .map(|j| serde_json::from_str(&j).unwrap())
        .collect();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["chat_id"], 13);
    assert_eq!(sent[0]["name"], "news");
    assert_eq!(sent[0]["@extra"], extra.0.to_string());
    // The other mutations send their constructors.
    driver
        .edit_forum_topic(ChatId(13), 5, "announcements")
        .unwrap()
        .expect("edit sent");
    driver
        .toggle_forum_topic_closed(ChatId(13), 5, true)
        .unwrap()
        .expect("close sent");
    driver
        .toggle_forum_topic_pinned(ChatId(13), 5, true)
        .unwrap()
        .expect("pin sent");
    driver
        .delete_forum_topic(ChatId(13), 5)
        .unwrap()
        .expect("delete sent");
    driver
        .toggle_general_forum_topic_hidden(ChatId(13), true)
        .unwrap()
        .expect("hide sent");
    let types: Vec<String> = recorder
        .snapshot()
        .into_iter()
        .map(|j| {
            serde_json::from_str::<Value>(&j).unwrap()["@type"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    for expected in [
        "editForumTopic",
        "toggleForumTopicIsClosed",
        "toggleForumTopicIsPinned",
        "deleteForumTopic",
        "toggleGeneralForumTopicIsHidden",
    ] {
        assert!(types.iter().any(|t| t == expected), "{expected} sent");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice G2: confirmed mutations refetch the dropped cache — forum
/// topic create (answers `forumTopicInfo`), welcome add (answers
/// `ok`), and `boostChat` (answers `chatBoostSlots`). A failed
/// mutation leaves the cache alone and refetches nothing.
#[test]
fn driver_mutation_confirmed_refetches_dropped_cache() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let ingest_json = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };
    ingest_json(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":false},"unread_count":0}}"#,
    );
    ingest_json(
        &mut driver,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"is_forum":true,"status":{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_manage_topics":true,"can_send_welcome_messages":true}}}}"#,
    );
    // Seed the caches the way the dialogs load them before mutating.
    driver.session.forum_topics.insert(13, vec![]);
    driver.session.welcome_messages.insert(13, vec![]);
    driver.session.chat_boost_status.insert(13, (0, 0));
    let sent_types = || {
        recorder
            .snapshot()
            .into_iter()
            .map(|j| {
                serde_json::from_str::<Value>(&j).unwrap()["@type"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect::<Vec<_>>()
    };

    // Forum create confirmed: cache dropped, `getForumTopics` refetch.
    let extra = driver
        .create_forum_topic(ChatId(13), "news")
        .unwrap()
        .expect("create sent");
    ingest_json(
        &mut driver,
        &format!(
            r#"{{"@type":"forumTopicInfo","@extra":"{}","chat_id":13}}"#,
            extra.0
        ),
    );
    assert!(!driver.session.forum_topics.contains_key(&13));
    assert!(
        sent_types().iter().any(|t| t == "getForumTopics"),
        "forum list refetched after confirmed create"
    );

    // Forum create failed: cache kept, nothing refetched.
    driver.session.forum_topics.insert(13, vec![]);
    let refetches_before = sent_types()
        .iter()
        .filter(|t| *t == "getForumTopics")
        .count();
    let extra = driver
        .create_forum_topic(ChatId(13), "news")
        .unwrap()
        .expect("create sent");
    ingest_json(
        &mut driver,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"TOPIC_INVALID"}}"#,
            extra.0
        ),
    );
    assert!(driver.session.forum_topics.contains_key(&13));
    assert_eq!(
        sent_types()
            .iter()
            .filter(|t| *t == "getForumTopics")
            .count(),
        refetches_before,
        "no refetch on failure"
    );

    // Welcome add confirmed: pack dropped, reload sent.
    let extra = driver
        .add_chat_welcome_message(ChatId(13), "hi")
        .unwrap()
        .expect("add sent");
    ingest_json(
        &mut driver,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(!driver.session.welcome_messages.contains_key(&13));
    assert!(
        sent_types().iter().any(|t| t == "loadChatWelcomeMessages"),
        "welcome pack reloaded after confirmed add"
    );

    // Boost confirmed: status dropped, `getChatBoostStatus` refetch.
    let slots_extra = driver
        .request_chat_boost(ChatId(13))
        .unwrap()
        .expect("slots sent");
    ingest_json(
        &mut driver,
        &format!(
            r#"{{"@type":"chatBoostSlots","@extra":"{}","slots":[{{"slot_id":3}}]}}"#,
            slots_extra.0
        ),
    );
    let boost_extra = driver
        .session
        .requests
        .pending_extra_for(RequestPurpose::BoostChat, Some(ChatId(13)))
        .expect("boostChat chained");
    ingest_json(
        &mut driver,
        &format!(
            r#"{{"@type":"chatBoostSlots","@extra":"{}","slots":[]}}"#,
            boost_extra.0
        ),
    );
    assert!(!driver.session.chat_boost_status.contains_key(&13));
    assert!(
        sent_types().iter().any(|t| t == "getChatBoostStatus"),
        "boost status refetched after confirmed boost"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice G2: `request_chat_boost` sends `getAvailableChatBoostSlots`;
/// the driver chains `boostChat` with the first slot id once the
/// answer arrives.
#[test]
fn driver_boost_chain_sends_slots_then_boost() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":13,"title":"c","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let extra = driver
        .request_chat_boost(ChatId(13))
        .unwrap()
        .expect("slots request sent");
    // A second intent while the first is in flight → no-op.
    assert_eq!(driver.request_chat_boost(ChatId(13)).unwrap(), None);
    let sent = recorder.snapshot();
    assert!(
        sent.iter()
            .any(|j| j.contains("getAvailableChatBoostSlots"))
    );
    // The slots answer chains `boostChat` with the first slot id.
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chatBoostSlots","@extra":"{}","slots":[{{"slot_id":3}},{{"slot_id":7}}]}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let boost = recorder
        .snapshot()
        .into_iter()
        .find(|j| j.contains(r#""@type":"boostChat""#))
        .expect("boostChat chained");
    let value: Value = serde_json::from_str(&boost).unwrap();
    assert_eq!(value["chat_id"], 13);
    assert_eq!(value["slot_ids"], serde_json::json!([3]));
    assert_eq!(driver.session.boost_intent, None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice G2: welcome-message send methods gate on the right and
/// send the right constructors.
#[test]
fn driver_welcome_message_mutations_send_and_gate() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let ingest_json = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };
    ingest_json(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":false},"unread_count":0}}"#,
    );
    ingest_json(
        &mut driver,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"status":{"@type":"chatMemberStatusMember"}}}"#,
    );
    // Plain member: gated out; empty text: rejected.
    assert_eq!(
        driver.add_chat_welcome_message(ChatId(13), "hi").unwrap(),
        None
    );
    assert!(driver.add_chat_welcome_message(ChatId(13), "  ").is_err());
    ingest_json(
        &mut driver,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"status":{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_send_welcome_messages":true}}}}"#,
    );
    driver
        .add_chat_welcome_message(ChatId(13), "welcome!")
        .unwrap()
        .expect("add sent");
    driver
        .edit_chat_welcome_message(ChatId(13), 7, "welcome back")
        .unwrap()
        .expect("edit sent");
    driver
        .delete_chat_welcome_message(ChatId(13), 7)
        .unwrap()
        .expect("delete sent");
    driver
        .load_chat_welcome_messages(ChatId(13))
        .unwrap()
        .expect("load sent");
    let types: Vec<String> = recorder
        .snapshot()
        .into_iter()
        .map(|j| {
            serde_json::from_str::<Value>(&j).unwrap()["@type"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    for expected in [
        "addChatWelcomeMessage",
        "editChatWelcomeMessage",
        "deleteChatWelcomeMessage",
        "loadChatWelcomeMessages",
    ] {
        assert!(types.iter().any(|t| t == expected), "{expected} sent");
    }
    let add = recorder
        .snapshot()
        .into_iter()
        .find(|j| j.contains("addChatWelcomeMessage"))
        .unwrap();
    let value: Value = serde_json::from_str(&add).unwrap();
    assert_eq!(value["chat_id"], 13);
    let _ = std::fs::remove_dir_all(&dir);
}

/// MED2: `recognize_speech` sends one `recognizeSpeech` for a real,
/// non-pending voice-note message; the chats path gate, unknown
/// messages, and pending messages are rejected (a refused request is
/// an error, never a faked transcript).
#[test]
fn driver_recognize_speech_sends_and_gates() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    // Chats path inactive before authorization is Ready: rejected.
    assert_eq!(
        driver.recognize_speech(ChatId(11), MessageId(90)),
        Err(ConnectSendError::InvalidRequest)
    );
    for json in [
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Cloud","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    // Unknown message: rejected.
    assert_eq!(
        driver.recognize_speech(ChatId(11), MessageId(90)),
        Err(ConnectSendError::InvalidRequest)
    );
    // A real voice-note message.
    let voice_json = r#"{"@type":"updateNewMessage","message":{"id":90,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageVoiceNote","voice_note":{"@type":"voiceNote","duration":3,"waveform":"","mime_type":"audio/ogg","voice":{"@type":"file","id":81,"size":10,"expected_size":10,"local":{"@type":"localFile","path":"","is_downloading_completed":false,"is_downloading_active":false,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"","unique_id":"","is_uploading_completed":false,"is_uploading_active":false,"uploaded_size":0}}}},"is_listened":false}}"#;
    driver
        .ingest(copy_and_parse(voice_json, &seq, &dyn_sink).unwrap())
        .unwrap();
    let extra = driver
        .recognize_speech(ChatId(11), MessageId(90))
        .expect("real message sends");
    let last = recorder.snapshot().into_iter().last().unwrap();
    let value: Value = serde_json::from_str(&last).unwrap();
    assert_eq!(value["@type"], "recognizeSpeech");
    assert_eq!(value["chat_id"], 11);
    assert_eq!(value["message_id"], 90);
    assert_eq!(value["@extra"], extra.0.to_string());
    // Pending (unsent) message: rejected.
    let history = driver.session.histories.get_mut(&11).unwrap();
    let mut pending_msg = history.messages.get(&90).unwrap().clone();
    pending_msg.id = MessageId(0);
    history.messages.insert(91, pending_msg);
    assert_eq!(
        driver.recognize_speech(ChatId(11), MessageId(91)),
        Err(ConnectSendError::InvalidRequest)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// MED2: `send_recorded_video_note` sends one `sendMessage` carrying
/// `inputMessageVideoNote` (duration + square length from the draft);
/// a closed chats path or an unsupported chat is rejected.
#[test]
fn driver_send_recorded_video_note_sends_input_message_video_note() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    // Chats path inactive before authorization is Ready: rejected.
    assert_eq!(
        driver.send_recorded_video_note(
            &crate::video::VideoNoteDraft {
                path: std::path::PathBuf::from("/nonexistent.mp4"),
                duration_secs: 5,
                length: 280,
            },
            None
        ),
        Err(ConnectSendError::InvalidRequest)
    );
    for json in [
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Cloud","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    // No open chat: rejected.
    assert_eq!(
        driver.send_recorded_video_note(
            &crate::video::VideoNoteDraft {
                path: std::path::PathBuf::from("/nonexistent.mp4"),
                duration_secs: 5,
                length: 280,
            },
            None
        ),
        Err(ConnectSendError::InvalidRequest)
    );
    driver.session.open_chat = Some(ChatId(11));
    // A real file so `pick_send_path` accepts it; the thumbnail probe
    // may fail on garbage bytes (thumbnail is optional).
    let clip = dir.join("clip.mp4");
    std::fs::write(&clip, b"not a real video").unwrap();
    let draft = crate::video::VideoNoteDraft {
        path: clip,
        duration_secs: 5,
        length: 280,
    };
    let extra = driver
        .send_recorded_video_note(&draft, None)
        .expect("sends");
    let last = recorder.snapshot().into_iter().last().unwrap();
    let value: Value = serde_json::from_str(&last).unwrap();
    assert_eq!(value["@type"], "sendMessage");
    assert_eq!(value["chat_id"], 11);
    assert_eq!(
        value["input_message_content"]["@type"],
        "inputMessageVideoNote"
    );
    assert_eq!(value["input_message_content"]["video_note"]["duration"], 5);
    assert_eq!(value["input_message_content"]["video_note"]["length"], 280);
    assert_eq!(value["@extra"], extra.0.to_string());
    let _ = std::fs::remove_dir_all(&dir);
}
