//! State reducer tests: messages.
use super::common::*;
use super::*;

#[test]
fn effective_preview_prefers_ephemeral_content() {
    // M2 fix-up: preview surfaces (reply-to header, chat-list snippet,
    // search hits, pinned/scheduled labels, notifications) must show the
    // ephemeral content instead of the regular content (schema 1.8.67,
    // line 3161).
    let parsed = ParsedMessage {
        sender: None,
        id: MessageId(602),
        chat_id: ChatId(14),
        date: 0,
        is_outgoing: false,
        is_pinned: false,
        topic_id: None,
        thread_id: None,
        ephemeral: Some(EphemeralMessageContent {
            content: Box::new(MessageContent::Text("secret flow".into())),
            reply_markup: None,
        }),
        media_album_id: 0,
        author_signature: None,
        scheduling_state: None,
        can_retry: false,
        send_state: Default::default(),
        content: MessageContent::Text("public".into()),
        files: Vec::new(),
        reply_to: None,
        forward_info: None,
        extras: Default::default(),
        interaction_info: None,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
    };
    assert_eq!(
        effective_preview(&history_message(parsed, false)),
        "secret flow"
    );
}

#[test]
fn send_success_replaces_pending_id() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(1));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":-1,"chat_id":1,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageSendSucceeded","old_message_id":-1,"message":{"id":88,"chat_id":1,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
    );
    let history = session.histories.get(&1).unwrap();
    assert!(!history.messages.contains_key(&-1));
    assert!(history.messages.contains_key(&88));
    assert!(!history.messages.get(&88).unwrap().pending);
}

#[test]
fn update_message_content_refreshes_scheduled_entry() {
    use crate::telegram::envelope::MessageSchedulingState;

    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.scheduled_messages.push(ParsedMessage {
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
        content: MessageContent::Text("old caption".into()),
        files: Vec::new(),
        reply_to: None,
        forward_info: None,
        extras: Default::default(),
        interaction_info: None,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
    });
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageContent","chat_id":7,"message_id":70,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"new caption","entities":[]}}}"#,
    );
    assert_eq!(
        session.scheduled_messages[0].content.preview(),
        "new caption"
    );
}

#[test]
fn send_failed_marks_can_retry_from_sending_state() {
    for (can_retry, label) in [(true, "retryable"), (false, "not retryable")] {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":-1,"chat_id":1,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateMessageSendFailed","old_message_id":-1,"message":{{"id":88,"chat_id":1,"is_outgoing":true,"sending_state":{{"@type":"messageSendingStateFailed","can_retry":{}}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}},"error":{{"code":400}}}}"#,
                can_retry,
            ),
        );
        let row = session
            .histories
            .get(&1)
            .unwrap()
            .messages
            .get(&88)
            .unwrap();
        assert!(row.failed, "{label}: failed send marks the row failed");
        assert_eq!(
            row.can_retry, can_retry,
            "{label}: can_retry parsed through"
        );
    }
}

#[test]
fn resend_error_surfaces_instead_of_silence() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::ResendMessages, Some(ChatId(1)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"MESSAGE_SEND_FAILED"}}"#,
            extra.0,
        ),
    );
    let err = session.resend_error.expect("resend error surfaced");
    assert!(err.contains("Could not retry the send"), "{err}");
}

#[test]
fn message_link_error_surfaces_instead_of_silence() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetMessageLink, Some(ChatId(1)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"LINK_NOT_AVAILABLE"}}"#,
            extra.0,
        ),
    );
    let err = session
        .message_link_error
        .expect("message link error surfaced");
    assert!(err.contains("Could not get message link"), "{err}");
}

#[test]
fn recognize_speech_error_surfaces_instead_of_silence() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::RecognizeSpeech, Some(ChatId(1)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"SPEECH_NOT_RECOGNIZED"}}"#,
            extra.0,
        ),
    );
    let err = session
        .recognize_speech_error
        .expect("recognize speech error surfaced");
    assert!(err.contains("Could not transcribe this message"), "{err}");
}

#[test]
fn permanent_delete_wins_over_old_fetch() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(1));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":9,"chat_id":1,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"bye","entities":[]}}}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateDeleteMessages","chat_id":1,"message_ids":[9],"is_permanent":true,"from_cache":false}"#,
    );
    let extra = session.request(RequestPurpose::GetHistory, Some(ChatId(1)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","messages":[{{"id":9,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"bye","entities":[]}}}}}}]}}"#,
            extra.0
        ),
    );
    assert!(!session.histories.get(&1).unwrap().messages.contains_key(&9));
    assert!(session.histories.get(&1).unwrap().tombstones.contains(&9));
}

#[test]
fn self_destructing_photo_disappears_via_delete_update() {
    use crate::telegram::envelope::SelfDestructKind;
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(41));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":901,"chat_id":41,"is_outgoing":false,"content":{"@type":"messagePhoto","photo":{"@type":"photo","has_stickers":false,"sizes":[]},"caption":{"@type":"formattedText","text":"","entities":[]},"show_caption_above_media":false,"has_spoiler":false,"is_secret":true},"self_destruct_type":{"@type":"messageSelfDestructTypeTimer","self_destruct_time":60},"self_destruct_in":42.5}}"#,
    );
    let history = session.histories.get(&41).unwrap();
    let row = history.messages.get(&901).unwrap();
    let sd = row.self_destruct.expect("timer parsed on history row");
    assert_eq!(sd.kind, SelfDestructKind::Timer { secs: 60 });
    assert!(row.has_live_self_destruct(unix_ms_now()));
    assert!(session.open_chat_has_live_self_destruct(unix_ms_now()));
    // The badge label comes from the latest `self_destruct_in`.
    let label = row
        .self_destruct_badge(sd.fetched_at_ms)
        .expect("badge for timer row");
    assert_eq!(label, "⏱ 43s left");
    // Timer fires server-side: the row leaves via `updateDeleteMessages`.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateDeleteMessages","chat_id":41,"message_ids":[901],"is_permanent":true,"from_cache":false}"#,
    );
    assert!(
        !session
            .histories
            .get(&41)
            .unwrap()
            .messages
            .contains_key(&901)
    );
    assert!(!session.open_chat_has_live_self_destruct(unix_ms_now()));
}

#[test]
fn open_chat_live_location_refresh_follows_running_shares_only() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(42));
    let live = |id: i64, chat: i64, expires_in: i32| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{chat},"is_outgoing":false,"content":{{"@type":"messageLiveLocation","location":{{"@type":"liveLocation","location":{{"@type":"location","latitude":1.0,"longitude":2.0,"horizontal_accuracy":0}},"live_period":900,"heading":0,"proximity_alert_radius":0}},"expires_in":{expires_in}}}}}}}"#
        )
    };
    let now = crate::local_time::now_unix();
    assert_eq!(session.open_chat_live_location_refresh(now), None);
    // A share in another chat does not count.
    apply_json(&mut session, &seq, &sink, &live(1, 43, 600));
    assert_eq!(session.open_chat_live_location_refresh(now), None);
    // An ended one (expires_in 0) does not either.
    apply_json(&mut session, &seq, &sink, &live(2, 42, 0));
    assert_eq!(session.open_chat_live_location_refresh(now), None);
    // A running one asks for a refresh within a minute; the soonest wins.
    apply_json(&mut session, &seq, &sink, &live(3, 42, 600));
    let wait = session
        .open_chat_live_location_refresh(now)
        .expect("running");
    assert!((1..=60).contains(&wait));
    apply_json(&mut session, &seq, &sink, &live(4, 42, 30));
    assert_eq!(session.open_chat_live_location_refresh(now), Some(1));
    // Long after both ended nothing is left to refresh.
    assert_eq!(session.open_chat_live_location_refresh(now + 10_000), None);
}

#[test]
fn auth_code_error_is_classified_without_native_message() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::CheckAuthenticationCode, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":400,"message":"PHONE_CODE_INVALID CANARY_CODE_999","@extra":"{}"}}"#,
            extra.0
        ),
    );
    let err = session
        .auth_state
        .last_auth_error
        .expect("classified auth error");
    assert_eq!(err.purpose, RequestPurpose::CheckAuthenticationCode);
    assert_eq!(err.class, ErrorClass::Invalid);
    assert_eq!(err.user_message(), "code not accepted");
    let logs = sink.rendered();
    assert!(!logs.contains("CANARY_CODE"));
    assert!(!logs.contains("PHONE_CODE_INVALID"));
    let debug = format!("{err:?}");
    assert!(!debug.contains("CANARY_CODE"));
}

#[test]
fn chat_action_typing_then_cancel() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatAction","chat_id":7,"topic_id":null,"sender_id":{"@type":"messageSenderUser","user_id":7},"action":{"@type":"chatActionTyping"}}"#,
    );
    let chat = session.chats.get(&7).unwrap();
    assert!(chat.is_peer_typing());
    assert_eq!(chat.sidebar_preview(), "typing");
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatAction","chat_id":7,"sender_id":{"@type":"messageSenderUser","user_id":7},"action":{"@type":"chatActionCancel"}}"#,
    );
    let chat = session.chats.get(&7).unwrap();
    assert!(!chat.is_peer_typing());
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatAction","chat_id":7,"sender_id":{"@type":"messageSenderUser","user_id":9},"action":{"@type":"chatActionRecordingVoiceNote"}}"#,
    );
    assert!(!session.chats.get(&7).unwrap().is_peer_typing());
}

#[test]
fn message_time_hhmm_formats_local_and_rejects_missing() {
    // 2026-09-28 21:42:00 UTC, in whatever zone the test host uses.
    let local = crate::local_time::civil_local(1790631720);
    assert_eq!(
        message_time_hhmm(1790631720),
        Some(format!("{:02}:{:02}", local.hour, local.minute))
    );
    assert_eq!(message_time_hhmm(0), None);
    assert_eq!(message_time_hhmm(-5), None);
}

#[test]
fn read_inbox_and_outbox_update_cursors() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":4,"title":"inbox","type":{"@type":"chatTypePrivate","user_id":4},"unread_count":3,"last_read_inbox_message_id":10,"last_read_outbox_message_id":0}}"#,
    );
    let chat = session.chats.get(&4).unwrap();
    assert_eq!(chat.unread_count, 3);
    assert_eq!(chat.last_read_inbox_message_id.0, 10);
    assert_eq!(chat.last_read_outbox_message_id.0, 0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatReadInbox","chat_id":4,"last_read_inbox_message_id":13,"unread_count":0}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatReadOutbox","chat_id":4,"last_read_outbox_message_id":12}"#,
    );
    let chat = session.chats.get(&4).unwrap();
    assert_eq!(chat.unread_count, 0);
    assert_eq!(chat.last_read_inbox_message_id.0, 13);
    assert_eq!(chat.last_read_outbox_message_id.0, 12);
    assert!(!sink.rendered().contains("CANARY"));
}

#[test]
fn outbox_receipt_is_honest_when_cursor_is_zero() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(1));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":1,"title":"dm","type":{"@type":"chatTypePrivate","user_id":1},"unread_count":0,"last_read_outbox_message_id":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":20,"chat_id":1,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_OUTBOX_hi","entities":[]}}}}"#,
    );
    let chat = session.chats.get(&1).unwrap();
    let message = session
        .histories
        .get(&1)
        .unwrap()
        .messages
        .get(&20)
        .unwrap();
    assert_eq!(chat.outbox_receipt(message), OutboxReceipt::Sent);
    assert_eq!(
        outgoing_status_label(false, chat.outbox_receipt(message)),
        "You · sent"
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatReadOutbox","chat_id":1,"last_read_outbox_message_id":20}"#,
    );
    let chat = session.chats.get(&1).unwrap();
    let message = session
        .histories
        .get(&1)
        .unwrap()
        .messages
        .get(&20)
        .unwrap();
    assert_eq!(chat.outbox_receipt(message), OutboxReceipt::Read);
    assert_eq!(
        outgoing_status_label(false, chat.outbox_receipt(message)),
        "You · read"
    );
    assert_eq!(
        outgoing_status_label(true, OutboxReceipt::None),
        "You (sending)"
    );
    assert!(!sink.rendered().contains("CANARY_OUTBOX"));
}

#[test]
fn view_messages_tdlib_error_releases_in_flight_ids() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(1));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":5,"chat_id":1,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
    );
    let extra = session.request(RequestPurpose::ViewMessages, Some(ChatId(1)));
    session.begin_viewing(ChatId(1), &[MessageId(5)]);
    assert!(session.message_ids_to_view(ChatId(1)).is_empty());
    assert!(
        session
            .requests
            .has_purpose_for_chat(RequestPurpose::ViewMessages, ChatId(1))
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":400,"message":"CANARY_VIEW_FAIL","@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert!(!session.requests.has_purpose(RequestPurpose::ViewMessages));
    assert_eq!(session.message_ids_to_view(ChatId(1)), vec![MessageId(5)]);
    assert!(session.histories.get(&1).unwrap().viewing.is_empty());
    assert!(!session.histories.get(&1).unwrap().viewed.contains(&5));
    assert!(!sink.rendered().contains("CANARY_VIEW_FAIL"));

    let extra = session.request(RequestPurpose::ViewMessages, Some(ChatId(1)));
    session.begin_viewing(ChatId(1), &[MessageId(5)]);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(session.message_ids_to_view(ChatId(1)).is_empty());
    assert!(session.histories.get(&1).unwrap().viewed.contains(&5));
    assert!(session.histories.get(&1).unwrap().viewing.is_empty());
}

#[test]
fn reported_visible_messages_survive_a_view_error() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(1));
    for id in [4, 5] {
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"m","entities":[]}}}}}}}}"#
            ),
        );
    }
    assert_eq!(session.message_ids_to_view(ChatId(1)), vec![MessageId(5)]);
    assert!(!session.report_visible_messages(ChatId(2), &[MessageId(4)]));
    assert!(session.report_visible_messages(ChatId(1), &[MessageId(4)]));
    assert_eq!(session.message_ids_to_view(ChatId(1)), vec![MessageId(4)]);
    let extra = session.request(RequestPurpose::ViewMessages, Some(ChatId(1)));
    session.begin_viewing(ChatId(1), &[MessageId(4)]);
    assert!(session.message_ids_to_view(ChatId(1)).is_empty());
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":400,"message":"x","@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert_eq!(session.message_ids_to_view(ChatId(1)), vec![MessageId(4)]);
    // Re-opening the chat starts over from the newest message.
    session.open_chat(ChatId(1));
    assert_eq!(session.message_ids_to_view(ChatId(1)), vec![MessageId(5)]);
}

#[test]
fn secret_photo_is_not_auto_thumbed() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(1));
    let file = media_file_json(3, "", false);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":12,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":true}}}}}}"#
        ),
    );
    assert!(session.thumb_file_ids_to_download().is_empty());
    assert!(session.should_download(FileId(3)));
}

#[test]
fn nested_idle_message_file_does_not_unstick_in_flight_download() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_download(FileId(6));
    session.begin_download(FileId(6));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &file_reply_json(extra.0, 6, true),
    );
    let file = media_file_json(6, "", false);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":13,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
        ),
    );
    assert!(session.downloading.contains(&6));
    assert!(!session.should_download(FileId(6)));
}

#[test]
fn reply_to_message_preview_and_jump() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":104,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"sounds good","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":11,"message_id":101,"quote":null,"checklist_task_id":0,"poll_option_id":""}}}"#,
    );
    session.open_chat(ChatId(11));
    let reply = session
        .histories
        .get(&11)
        .unwrap()
        .messages
        .get(&104)
        .unwrap();
    assert_eq!(
        reply.reply_to.as_ref().map(|r| r.message_id),
        Some(MessageId(101))
    );
    assert_eq!(
        session.reply_quote_preview(reply).as_deref(),
        Some("Hello from injected JSON.")
    );
    assert_eq!(
        session.begin_chat_search_jump(reply.reply_to.as_ref().unwrap().message_id),
        ChatSearchJumpNeed::AlreadyReady
    );
    assert_eq!(
        session.chat_search.jump,
        ChatSearchJump::Ready {
            message_id: MessageId(101)
        }
    );

    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":105,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"quoted","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":11,"message_id":90,"quote":{"@type":"textQuote","text":{"@type":"formattedText","text":"manual quote","entities":[]},"position":0,"is_manual":true},"checklist_task_id":0,"poll_option_id":""}}}"#,
    );
    let quoted = session
        .histories
        .get(&11)
        .unwrap()
        .messages
        .get(&105)
        .unwrap();
    assert_eq!(
        session.reply_quote_preview(quoted).as_deref(),
        Some("manual quote")
    );
    assert_eq!(
        session.begin_chat_search_jump(MessageId(90)),
        ChatSearchJumpNeed::LoadAround
    );
    let around = session.request_history_around(ChatId(11), MessageId(90));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{{"id":90,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older original","entities":[]}}}}}}]}}"#,
            around.0
        ),
    );
    assert_eq!(
        session.chat_search.jump,
        ChatSearchJump::Ready {
            message_id: MessageId(90)
        }
    );
    assert!(session.histories.get(&11).unwrap().contains(MessageId(90)));
}

#[test]
fn update_message_content_rewrites_own_text() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Demo","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":102,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Reply from the session reducer.","entities":[]}}}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageContent","chat_id":11,"message_id":102,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_EDITED_own","entities":[]}}}"#,
    );
    let text = session
        .histories
        .get(&11)
        .unwrap()
        .messages
        .get(&102)
        .unwrap()
        .content
        .preview();
    assert_eq!(text, "CANARY_EDITED_own");
    assert!(
        !session
            .histories
            .get(&11)
            .unwrap()
            .is_tombstone(MessageId(102))
    );
    assert!(!sink.rendered().contains("CANARY_EDITED"));
}
