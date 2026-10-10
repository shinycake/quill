//! Driver tests: editing, deleting, forwarding and reacting to messages.
use super::*;

#[test]
fn driver_edit_text_shape_and_incoming_rejected() {
    use crate::composer::{ComposerEdit, ComposerEditKind};

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
                    r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"own outgoing","entities":[]}}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    let incoming = ComposerEdit {
        chat_id: ChatId(7),
        message_id: MessageId(50),
        original_text: "hello already here".into(),
        kind: ComposerEditKind::Text,
        scheduled: false,
        caption_above: false,
        media_edit: Default::default(),
        link_preview: Default::default(),
    };
    assert_eq!(
        driver.edit_snapshot(&incoming, "nope"),
        Err(ConnectSendError::InvalidRequest)
    );

    let edit = ComposerEdit {
        chat_id: ChatId(7),
        message_id: MessageId(60),
        original_text: "own outgoing".into(),
        kind: ComposerEditKind::Text,
        scheduled: false,
        caption_above: false,
        media_edit: Default::default(),
        link_preview: Default::default(),
    };
    assert_eq!(
        driver.edit_snapshot(&edit, "   "),
        Err(ConnectSendError::InvalidRequest)
    );
    let extra = driver.edit_snapshot(&edit, "CANARYEDITtext").unwrap();
    let json = recorder
        .snapshot()
        .last()
        .cloned()
        .expect("editMessageText");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editMessageText");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["message_id"], 60);
    assert_eq!(v["reply_markup"], Value::Null);
    assert_eq!(v["input_message_content"]["@type"], "inputMessageText");
    assert_eq!(v["input_message_content"]["text"]["text"], "CANARYEDITtext");
    assert_eq!(v["input_message_content"]["clear_draft"], false);

    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageContent","chat_id":7,"message_id":60,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARYEDITtext","entities":[]}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(
        driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .get(&60)
            .unwrap()
            .content
            .preview(),
        "CANARYEDITtext"
    );
    assert!(!sink.rendered().contains("CANARYEDIT"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_edit_scheduled_message_uses_scheduled_list() {
    use crate::composer::{ComposerEdit, ComposerEditKind};
    use crate::ids::{ChatId, MessageId};
    use crate::telegram::envelope::{
        MessageContent, MessageSchedulingState, ParsedMessage, TextContent,
    };

    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    // A scheduled send lives in `session.scheduled_messages`, not history.
    driver.session.scheduled_messages.push(ParsedMessage {
        sender: None,
        id: MessageId(70),
        chat_id: ChatId(7),
        date: 0,
        is_outgoing: true,
        is_pinned: false,
        topic_id: None,
        thread_id: None,
        ephemeral: None,
        media_album_id: 0,
        author_signature: None,
        scheduling_state: Some(MessageSchedulingState::SendAtDate { send_date: 999 }),
        can_retry: false,
        send_state: Default::default(),
        content: MessageContent::Text(TextContent::plain("scheduled draft")),
        files: Vec::new(),
        reply_to: None,
        forward_info: None,
        extras: Default::default(),
        interaction_info: None,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
    });

    // A non-scheduled edit for the same id finds nothing in history.
    let plain = ComposerEdit {
        chat_id: ChatId(7),
        message_id: MessageId(70),
        original_text: "scheduled draft".into(),
        kind: ComposerEditKind::Text,
        scheduled: false,
        caption_above: false,
        media_edit: Default::default(),
        link_preview: Default::default(),
    };
    assert_eq!(
        driver.edit_snapshot(&plain, "nope"),
        Err(ConnectSendError::InvalidRequest)
    );

    // The scheduled edit validates against the scheduled list and sends
    // the same `editMessageText` request.
    let scheduled = ComposerEdit {
        scheduled: true,
        ..plain
    };
    let extra = driver.edit_snapshot(&scheduled, "CANARYSCHEDedit").unwrap();
    let json = recorder
        .snapshot()
        .last()
        .cloned()
        .expect("editMessageText");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editMessageText");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["message_id"], 70);
    assert_eq!(
        v["input_message_content"]["text"]["text"],
        "CANARYSCHEDedit"
    );
    assert!(!sink.rendered().contains("CANARYSCHED"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_delete_confirm_shape_and_tombstone() {
    use crate::composer::DeleteConfirm;

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
                    r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"own outgoing","entities":[]}}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    let incoming = DeleteConfirm {
        chat_id: ChatId(7),
        message_id: MessageId(50),
        revoke: false,
        can_revoke: false,
    };
    // M1: incoming messages are deletable for the current user
    // (`revoke: false`); the for-everyone toggle degrades to for-me.
    let extra = driver.delete_confirmed(&incoming).unwrap();
    let json = recorder.snapshot().last().cloned().expect("deleteMessages");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "deleteMessages");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["message_ids"], serde_json::json!([50]));
    assert_eq!(v["revoke"], false);

    let mut incoming_revoke = incoming.clone();
    incoming_revoke.revoke = true;
    let extra = driver.delete_confirmed(&incoming_revoke).unwrap();
    let json = recorder.snapshot().last().cloned().expect("deleteMessages");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["message_ids"], serde_json::json!([50]));
    assert_eq!(v["revoke"], false, "revoke is never sent for incoming");

    let missing = DeleteConfirm {
        chat_id: ChatId(7),
        message_id: MessageId(999),
        revoke: false,
        can_revoke: false,
    };
    assert_eq!(
        driver.delete_confirmed(&missing),
        Err(ConnectSendError::InvalidRequest)
    );

    // Selection mode: one request for several messages; "delete for
    // everyone" only when they're all your own.
    driver
        .delete_selected(ChatId(7), &[MessageId(50), MessageId(60)], true)
        .unwrap();
    let v: Value = serde_json::from_str(&recorder.snapshot().last().cloned().unwrap()).unwrap();
    assert_eq!(v["message_ids"], serde_json::json!([50, 60]));
    assert_eq!(v["revoke"], false, "a mixed selection deletes only for me");
    driver
        .delete_selected(ChatId(7), &[MessageId(60)], true)
        .unwrap();
    let v: Value = serde_json::from_str(&recorder.snapshot().last().cloned().unwrap()).unwrap();
    assert_eq!(v["revoke"], true);
    assert_eq!(
        driver.delete_selected(ChatId(7), &[MessageId(60), MessageId(999)], true),
        Err(ConnectSendError::InvalidRequest)
    );

    let confirm = DeleteConfirm::own(ChatId(7), MessageId(60), true, false).unwrap();
    let extra = driver.delete_confirmed(&confirm).unwrap();
    let json = recorder.snapshot().last().cloned().expect("deleteMessages");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "deleteMessages");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["message_ids"], serde_json::json!([60]));
    assert_eq!(v["revoke"], true);

    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateDeleteMessages","chat_id":7,"message_ids":[60],"is_permanent":true,"from_cache":false}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let history = driver.session.histories.get(&7).unwrap();
    assert!(!history.contains(MessageId(60)));
    assert!(history.is_tombstone(MessageId(60)));
    assert!(!sink.rendered().contains("CANARY"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_forward_messages_shape_and_dest_result() {
    use crate::composer::ForwardDraft;

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
                    r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatPosition","chat_id":8,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"8","is_pinned":false}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"own outgoing","entities":[]}}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    let mut draft = ForwardDraft::from_message(ChatId(7), MessageId(50), false).unwrap();
    draft.toggle(ChatId(7), MessageId(60), false);
    let extra = driver.forward_messages(ChatId(8), &draft).unwrap();
    let json = recorder
        .snapshot()
        .last()
        .cloned()
        .expect("forwardMessages");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "forwardMessages");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 8);
    assert_eq!(v["from_chat_id"], 7);
    assert_eq!(v["topic_id"], Value::Null);
    assert_eq!(v["message_ids"], serde_json::json!([50, 60]));
    assert_eq!(v["send_copy"], false);
    assert_eq!(v["remove_caption"], false);
    assert_eq!(v["options"], Value::Null);

    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{{"id":80,"chat_id":8,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hello already here","entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":7}},"date":1}}}},{{"id":81,"chat_id":8,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"own outgoing","entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":7}},"date":1}}}}]}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let result = driver
        .session
        .last_forward
        .as_ref()
        .expect("forward result");
    assert_eq!(result.dest_title, "Bob");
    assert_eq!(result.forwarded_ids, vec![MessageId(80), MessageId(81)]);
    assert_eq!(result.success_label(), "Forwarded 2 messages to Bob");
    assert!(
        driver
            .session
            .histories
            .get(&8)
            .unwrap()
            .contains(MessageId(80))
    );
    assert_eq!(
        driver.session.forward_from_label(
            driver
                .session
                .histories
                .get(&8)
                .unwrap()
                .messages
                .get(&80)
                .unwrap()
                .forward_info
                .as_ref()
                .unwrap()
        ),
        "Forwarded from Alice"
    );
    assert!(!sink.rendered().contains("CANARY"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_add_and_remove_message_reaction_then_interaction_info() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    let extra = driver
        .toggle_message_reaction(ChatId(7), MessageId(50), "❤")
        .unwrap();
    let json = recorder
        .snapshot()
        .last()
        .cloned()
        .expect("addMessageReaction");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "addMessageReaction");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["message_id"], 50);
    assert_eq!(v["reaction_type"]["@type"], "reactionTypeEmoji");
    assert_eq!(v["reaction_type"]["emoji"], "❤");
    assert_eq!(v["is_big"], false);
    assert_eq!(v["update_recent_reactions"], true);
    assert!(!json.contains("setMessageReactions"));

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
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageInteractionInfo","chat_id":7,"message_id":50,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"total_count":1,"is_chosen":true,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let reacted = driver
        .session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&50)
        .unwrap();
    assert!(reacted.chosen_emoji("❤"));
    assert_eq!(
        reacted.emoji_reaction_chips()[0].chip_label().as_deref(),
        Some("❤ 1")
    );

    let remove_extra = driver
        .toggle_message_reaction(ChatId(7), MessageId(50), "❤")
        .unwrap();
    let remove_json = recorder
        .snapshot()
        .last()
        .cloned()
        .expect("removeMessageReaction");
    let v: Value = serde_json::from_str(&remove_json).unwrap();
    assert_eq!(v["@type"], "removeMessageReaction");
    assert_eq!(v["@extra"], remove_extra.0.to_string());
    assert_eq!(v["reaction_type"]["emoji"], "❤");

    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageInteractionInfo","chat_id":7,"message_id":50,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":null}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let cleared = driver
        .session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&50)
        .unwrap();
    assert!(cleared.emoji_reaction_chips().is_empty());
    assert!(!sink.rendered().contains("CANARY"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_edit_scheduled_message_sends_scheduling_state() {
    use crate::composer::ComposerScheduling;
    use crate::ids::{ChatId, MessageId};
    use crate::telegram::envelope::{
        MessageContent, MessageSchedulingState, ParsedMessage, TextContent,
    };

    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    driver.session.scheduled_messages.push(ParsedMessage {
        sender: None,
        id: MessageId(70),
        chat_id: ChatId(7),
        date: 0,
        is_outgoing: true,
        is_pinned: false,
        topic_id: None,
        thread_id: None,
        ephemeral: None,
        media_album_id: 0,
        author_signature: None,
        scheduling_state: Some(MessageSchedulingState::SendAtDate { send_date: 999 }),
        can_retry: false,
        send_state: Default::default(),
        content: MessageContent::Text(TextContent::plain("later")),
        files: Vec::new(),
        reply_to: None,
        forward_info: None,
        extras: Default::default(),
        interaction_info: None,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
    });
    // Unknown ids are refused before anything is sent.
    assert_eq!(
        driver.edit_scheduled_message(ChatId(7), MessageId(71), ComposerScheduling::None),
        Err(ConnectSendError::InvalidRequest)
    );
    let extra = driver
        .edit_scheduled_message(
            ChatId(7),
            MessageId(70),
            ComposerScheduling::SendAtDate(1_800_000_600),
        )
        .unwrap();
    let json = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editMessageSchedulingState");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["scheduling_state"]["send_date"], 1_800_000_600);
    let _ = std::fs::remove_dir_all(&dir);
}
