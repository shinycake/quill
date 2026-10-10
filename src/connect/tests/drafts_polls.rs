//! Connect-driver tests: drafts, typing, polls.
use super::super::*;
use super::*;
use crate::composer::ComposerSnapshot;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::MessageContent;
use crate::telegram::requests::SendReply;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn driver_mute_forever_then_unmute_and_archive() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    let extra = driver.mute_chat_forever(ChatId(7)).unwrap();
    let json = recorder
        .snapshot()
        .last()
        .cloned()
        .expect("setChatNotificationSettings");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setChatNotificationSettings");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 7);
    assert_eq!(
        v["notification_settings"]["@type"],
        "chatNotificationSettings"
    );
    assert_eq!(v["notification_settings"]["use_default_mute_for"], false);
    assert_eq!(v["notification_settings"]["mute_for"], i32::MAX);
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
                    r#"{"@type":"updateChatNotificationSettings","chat_id":7,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":false,"mute_for":2147483647,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":false,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":true,"show_story_poster":false,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert!(driver.session.chats.get(&7).unwrap().is_muted());

    let unmute = driver.unmute_chat(ChatId(7)).unwrap();
    let unmute_json = recorder.snapshot().last().cloned().unwrap();
    let v: Value = serde_json::from_str(&unmute_json).unwrap();
    assert_eq!(v["@type"], "setChatNotificationSettings");
    assert_eq!(v["@extra"], unmute.0.to_string());
    assert_eq!(v["notification_settings"]["mute_for"], 0);

    let archive = driver.archive_chat(ChatId(7)).unwrap();
    let archive_json = recorder.snapshot().last().cloned().unwrap();
    let v: Value = serde_json::from_str(&archive_json).unwrap();
    assert_eq!(v["@type"], "addChatToList");
    assert_eq!(v["@extra"], archive.0.to_string());
    assert_eq!(v["chat_list"]["@type"], "chatListArchive");
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"0","is_pinned":false}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"4","is_pinned":false}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert!(driver.session.ordered_chats().is_empty());
    assert_eq!(driver.session.ordered_archived_chats()[0].id.0, 7);

    let unarchive = driver.unarchive_chat(ChatId(7)).unwrap();
    let unarchive_json = recorder.snapshot().last().cloned().unwrap();
    let v: Value = serde_json::from_str(&unarchive_json).unwrap();
    assert_eq!(v["@type"], "addChatToList");
    assert_eq!(v["@extra"], unarchive.0.to_string());
    assert_eq!(v["chat_list"]["@type"], "chatListMain");
    assert!(!sink.rendered().contains("CANARY"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_sends_typing_then_cancel_on_empty_and_send() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    driver.sync_outgoing_typing("hello", false, 1_000).unwrap();
    let typing = recorder.snapshot().last().cloned().expect("sendChatAction");
    let v: Value = serde_json::from_str(&typing).unwrap();
    assert_eq!(v["@type"], "sendChatAction");
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["topic_id"], Value::Null);
    assert_eq!(v["business_connection_id"], "");
    assert_eq!(v["action"]["@type"], "chatActionTyping");

    let before = recorder.snapshot().len();
    driver.sync_outgoing_typing("hello!", false, 2_000).unwrap();
    assert_eq!(recorder.snapshot().len(), before, "4s throttle");

    driver
        .sync_outgoing_typing("hello!!", false, 1_000 + OUTGOING_TYPING_INTERVAL_MS)
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(recorder.snapshot().last().unwrap()).unwrap()["action"]["@type"],
        "chatActionTyping"
    );

    driver.sync_outgoing_typing("   ", false, 9_000).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(recorder.snapshot().last().unwrap()).unwrap()["action"]["@type"],
        "chatActionCancel"
    );

    driver.sync_outgoing_typing("again", false, 10_000).unwrap();
    let snap = ComposerSnapshot::capture(ChatId(7), driver.session.view_generation, "again");
    driver.send_text_snapshot(&snap).unwrap();
    let sent = recorder.snapshot();
    assert_eq!(
        serde_json::from_str::<Value>(&sent[sent.len() - 2]).unwrap()["@type"],
        "sendMessage"
    );
    assert_eq!(
        serde_json::from_str::<Value>(sent.last().unwrap()).unwrap()["action"]["@type"],
        "chatActionCancel"
    );

    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatAction","chat_id":7,"topic_id":null,"sender_id":{"@type":"messageSenderUser","user_id":7},"action":{"@type":"chatActionTyping"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert!(driver.session.chats.get(&7).unwrap().is_peer_typing());
    assert!(!sink.rendered().contains("CANARY"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn private_draft_debounces_then_flushes_and_skips_channels() {
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
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0,"draft_message":{"@type":"draftMessage","reply_to":{"@type":"inputMessageReplyToMessage","message_id":3,"quote":null,"checklist_task_id":0,"poll_option_id":""},"date":1,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"meet at 6","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":8,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":8,"is_channel":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let restored = driver.session.chats.get(&7).unwrap().draft.clone().unwrap();
    assert_eq!(restored.text, "meet at 6");
    assert_eq!(restored.reply_to_message_id, Some(MessageId(3)));
    let outcome = driver
        .note_composer_draft(ChatId(7), "meet at 6!", None, 1_000, true)
        .unwrap();
    let DraftSaveOutcome::Debounced { token, delay } = outcome else {
        panic!("expected debounce");
    };
    assert_eq!(delay, DRAFT_SAVE_DEBOUNCE);
    assert!(
        driver
            .commit_debounced_draft(token.wrapping_add(9))
            .unwrap()
            .is_none()
    );
    driver.commit_debounced_draft(token).unwrap();
    let sent = recorder.snapshot();
    let draft_json = sent.last().unwrap();
    assert!(draft_json.contains("setChatDraftMessage"));
    assert!(draft_json.contains("meet at 6!"));
    assert!(draft_json.contains("\"topic_id\":null"));
    assert_eq!(
        driver
            .session
            .chats
            .get(&7)
            .unwrap()
            .draft
            .as_ref()
            .unwrap()
            .text,
        "meet at 6!"
    );
    let flushed = driver
        .note_composer_draft(
            ChatId(7),
            "leaving",
            Some(SendReply::plain(MessageId(3))),
            2_000,
            false,
        )
        .unwrap();
    assert_eq!(flushed, DraftSaveOutcome::Sent);
    assert!(
        driver
            .note_composer_draft(ChatId(8), "nope", None, 3_000, false)
            .unwrap()
            == DraftSaveOutcome::Skipped
    );
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageSendSucceeded","message":{"id":9,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"leaving","entities":[]}}},"old_message_id":-1}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(driver.session.draft_clears, vec![ChatId(7)]);
    driver.clear_draft_after_send(ChatId(7), true).unwrap();
    assert!(driver.session.chats.get(&7).unwrap().draft.is_none());
    assert!(!sink.rendered().contains("CANARY"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn quoted_draft_round_trips_input_text_quote() {
    // Slice G1: a composer reply carrying a validated partial quote is
    // saved to the server draft as `inputTextQuote` (schema 1.8.67
    // line 3056) and kept in the local `ChatDraft` for restore.
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
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver.select_chat(ChatId(7)).unwrap();
    let reply = SendReply {
        message_id: MessageId(3),
        quote: Some(("meet at".to_string(), 0)),
        source_chat: None,
    };
    let outcome = driver
        .note_composer_draft(ChatId(7), "sounds good", Some(reply), 1_000, false)
        .unwrap();
    assert_eq!(outcome, DraftSaveOutcome::Sent);
    let sent = recorder.snapshot();
    let draft_json = sent.last().expect("draft request sent");
    assert!(draft_json.contains("setChatDraftMessage"));
    assert!(draft_json.contains("\"inputTextQuote\""));
    assert!(draft_json.contains("meet at"));
    let saved = driver.session.chats.get(&7).unwrap().draft.clone().unwrap();
    assert_eq!(saved.text, "sounds good");
    assert_eq!(saved.reply_to_message_id, Some(MessageId(3)));
    assert_eq!(saved.quote, Some(("meet at".to_string(), 0)));
    assert!(!sink.rendered().contains("CANARY"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn search_open_flushes_leaving_draft_and_media_send_drops_reply() {
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
    for (id, title) in [(7, "Ada"), (8, "Bob")] {
        driver
                .ingest(
                    copy_and_parse(
                        &format!(
                            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0}}}}"#
                        ),
                        &seq,
                        &dyn_sink,
                    )
                    .unwrap(),
                )
                .unwrap();
    }
    driver.select_chat(ChatId(7)).unwrap();
    let outcome = driver
        .note_composer_draft(
            ChatId(7),
            "hello",
            Some(SendReply::plain(MessageId(3))),
            1_000,
            true,
        )
        .unwrap();
    assert!(matches!(outcome, DraftSaveOutcome::Debounced { .. }));
    driver
        .select_search_chat(
            ChatId(8),
            "hello",
            Some(SendReply::plain(MessageId(3))),
            1_500,
        )
        .unwrap();
    assert_eq!(driver.session.open_chat, Some(ChatId(8)));
    let saved = driver.session.chats.get(&7).unwrap().draft.clone().unwrap();
    assert_eq!(saved.text, "hello");
    assert_eq!(saved.reply_to_message_id, Some(MessageId(3)));
    let sent = recorder.snapshot();
    assert!(sent.iter().any(|json| {
        json.contains("setChatDraftMessage")
            && json.contains("\"chat_id\":7")
            && json.contains("hello")
            && json.contains("\"message_id\":3")
    }));
    driver.select_chat(ChatId(7)).unwrap();
    driver
        .note_composer_draft(
            ChatId(7),
            "caption",
            Some(SendReply::plain(MessageId(3))),
            2_000,
            false,
        )
        .unwrap();
    driver
        .note_composer_draft(ChatId(7), "caption", None, 2_100, false)
        .unwrap();
    let after = driver.session.chats.get(&7).unwrap().draft.clone().unwrap();
    assert_eq!(after.text, "caption");
    assert_eq!(after.reply_to_message_id, None);
    driver
        .note_composer_draft(
            ChatId(7),
            "  ",
            Some(SendReply::plain(MessageId(9))),
            3_000,
            false,
        )
        .unwrap();
    driver
        .select_search_message(ChatId(8), MessageId(1), "  ", None, 3_100)
        .unwrap();
    assert!(driver.session.chats.get(&7).unwrap().draft.is_none());
    assert!(!sink.rendered().contains("CANARY"));
    let _ = std::fs::remove_dir_all(&dir);
}

pub(crate) const POLL_OPEN_JSON: &str = r#"{"@type":"updateNewMessage","message":{"id":106,"chat_id":7,"is_outgoing":false,"content":{"@type":"messagePoll","poll":{"@type":"poll","id":9001,"question":{"@type":"formattedText","text":"Lunch?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Sushi","entities":[]},"voter_count":12,"vote_percentage":55,"is_chosen":false},{"@type":"pollOption","id":"b","text":{"@type":"formattedText","text":"Pizza","entities":[]},"voter_count":7,"vote_percentage":32,"is_chosen":false}],"total_voter_count":19,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"type":{"@type":"pollTypeRegular"}},"description":{"@type":"formattedText","text":"","entities":[]},"can_add_option":false}}}"#;

pub(crate) const POLL_CLOSED_JSON: &str = r#"{"@type":"updateNewMessage","message":{"id":107,"chat_id":7,"is_outgoing":false,"content":{"@type":"messagePoll","poll":{"@type":"poll","id":9002,"question":{"@type":"formattedText","text":"Red planet?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Mars","entities":[]},"voter_count":18,"vote_percentage":72,"is_chosen":false}],"total_voter_count":25,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":false,"is_closed":true,"type":{"@type":"pollTypeQuiz","correct_option_ids":[0],"explanation":{"@type":"formattedText","text":"","entities":[]}}},"description":{"@type":"formattedText","text":"","entities":[]},"can_add_option":false}}}"#;

#[test]
fn driver_poll_answer_rejects_when_chats_path_inactive() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink);
    // No auth state seeded: `chats_path_active()` is false.
    let mut driver = ConnectDriver::new(session, recorder, test_credentials(), prepared);
    assert_invalid(driver.send_poll_answer(ChatId(7), MessageId(106), 0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_poll_answer_rejects_unsupported_chat() {
    let (dir, mut driver, _recorder, sink, dyn_sink, seq) = poll_driver();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Secret","type":{"@type":"chatTypeSecret","secret_chat_id":9},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_invalid(driver.send_poll_answer(ChatId(9), MessageId(106), 0));
    assert!(!sink.rendered().contains("CANARY"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_poll_answer_guards_message_poll_and_index() {
    let (dir, mut driver, recorder, sink, _dyn_sink, _seq) = poll_driver();
    // Missing message.
    assert_invalid(driver.send_poll_answer(ChatId(7), MessageId(404), 0));
    // Not a poll (message 50 is Alice's text message).
    assert_invalid(driver.send_poll_answer(ChatId(7), MessageId(50), 0));
    // Closed poll (quiz, message 107).
    assert_invalid(driver.send_poll_answer(ChatId(7), MessageId(107), 0));
    // Out-of-range option index.
    assert_invalid(driver.send_poll_answer(ChatId(7), MessageId(106), 9));
    // Valid single-answer tap: `setPollAnswer` with the 0-based position,
    // plus an optimistic chosen mark before `updatePoll` arrives.
    let extra = driver
        .send_poll_answer(ChatId(7), MessageId(106), 0)
        .unwrap();
    let send_json = recorder.snapshot().last().cloned().expect("setPollAnswer");
    let v: Value = serde_json::from_str(&send_json).unwrap();
    assert_eq!(v["@type"], "setPollAnswer");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["message_id"], 106);
    assert_eq!(v["option_ids"], serde_json::json!([0]));
    let history = driver.session.histories.get(&7).unwrap();
    let message = history.messages.get(&106).unwrap();
    match &message.content {
        MessageContent::Poll(poll) => {
            assert!(poll.poll.options[0].is_chosen);
            assert!(!poll.poll.options[1].is_chosen);
        }
        other => panic!("{other:?}"),
    }
    assert!(!sink.rendered().contains("CANARY"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_poll_draft_guards_and_request_shape() {
    use crate::poll::PollDraft;

    let (dir, mut driver, recorder, sink, dyn_sink, seq) = poll_driver();
    let valid = PollDraft {
        question: "Lunch?".into(),
        description: "team vote".into(),
        options: vec!["Sushi".into(), "Pizza".into()],
        is_anonymous: true,
        allows_multiple_answers: false,
        allows_revoting: false,
        shuffle_options: true,
        duration_hours: "2".into(),
        country_codes: vec!["US".into()],
        ..Default::default()
    };
    let invalid = PollDraft {
        question: "  ".into(),
        options: vec!["Sushi".into(), "Pizza".into()],
        is_anonymous: true,
        allows_multiple_answers: false,
        ..Default::default()
    };
    // Invalid draft rejected before any request is built.
    assert_invalid(driver.send_poll_draft(ChatId(7), &invalid, None));
    // Admin-gated broadcast channel: `can_post()` is false.
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":13,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_invalid(driver.send_poll_draft(ChatId(13), &valid, None));
    // Valid draft: `sendMessage` + `inputMessagePoll`.
    let extra = driver
        .send_poll_draft(ChatId(7), &valid, Some(SendReply::plain(MessageId(50))))
        .unwrap();
    let send_json = recorder.snapshot().last().cloned().expect("sendMessage");
    let v: Value = serde_json::from_str(&send_json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["reply_to"]["message_id"], 50);
    let content = &v["input_message_content"];
    assert_eq!(content["@type"], "inputMessagePoll");
    assert_eq!(content["question"]["text"], "Lunch?");
    assert_eq!(content["description"]["text"], "team vote");
    assert_eq!(content["options"][0]["text"]["text"], "Sushi");
    assert_eq!(content["allows_revoting"], false);
    assert_eq!(content["shuffle_options"], true);
    assert_eq!(content["country_codes"], serde_json::json!(["US"]));
    assert_eq!(content["open_period"], 2 * 3600);
    assert_eq!(content["type"]["@type"], "inputPollTypeRegular");
    assert!(!sink.rendered().contains("CANARY"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_quiz_draft_sends_quiz_type() {
    use crate::poll::PollDraft;

    let (dir, mut driver, recorder, sink, _dyn_sink, _seq) = poll_driver();
    let quiz = PollDraft {
        question: "Capital of France?".into(),
        options: vec!["Paris".into(), "London".into()],
        is_quiz: true,
        quiz_correct: Some(0),
        quiz_explanation: "think Eiffel".into(),
        allows_multiple_answers: true, // normalized off for quizzes
        allows_revoting: true,         // normalized off for quizzes
        ..Default::default()
    };
    assert!(quiz.validate().is_none());
    driver
        .send_poll_draft(ChatId(7), &quiz, None)
        .expect("valid quiz draft sends");
    let send_json = recorder.snapshot().last().cloned().expect("sendMessage");
    let v: Value = serde_json::from_str(&send_json).unwrap();
    let content = &v["input_message_content"];
    assert_eq!(content["type"]["@type"], "inputPollTypeQuiz");
    assert_eq!(
        content["type"]["correct_option_ids"],
        serde_json::json!([0])
    );
    assert_eq!(content["type"]["explanation"]["text"], "think Eiffel");
    assert!(content["type"]["explanation_media"].is_null());
    assert_eq!(content["allows_multiple_answers"], false);
    assert_eq!(content["allows_revoting"], false);
    assert!(!sink.rendered().contains("CANARY"));
    let _ = std::fs::remove_dir_all(&dir);
}
