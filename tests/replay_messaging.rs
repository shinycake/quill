//! Core messaging replay tests.
//! Split from `tests/replay.rs` — pure code motion.
mod replay_common;
use replay_common::*;

#[test]
fn replay_send_interleaving() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let extra = session.request(RequestPurpose::SendMessage, Some(quill::ids::ChatId(7)));
    apply_all(
        &mut session,
        &sink,
        &[
            &format!(
                r#"{{"@type":"message","@extra":"{}","id":-42,"chat_id":7,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"ping","entities":[]}}}}}}"#,
                extra.0
            ),
            r#"{"@type":"updateMessageSendAcknowledged","chat_id":7,"message_id":-42}"#,
            r#"{"@type":"updateMessageSendSucceeded","old_message_id":-42,"message":{"id":1001,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"ping","entities":[]}}}}"#,
        ],
    );
    let history = session.histories.get(&7).unwrap();
    assert!(!history.messages.contains_key(&-42));
    assert_eq!(history.messages.get(&1001).unwrap().id.0, 1001);
}

#[test]
fn replay_ready_load_chats_select_and_send() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            r#"{"@type":"updateChatLastMessage","chat_id":7,"last_message":{"id":1,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"yo","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}]}"#,
        ],
    );
    assert!(matches!(
        session.auth,
        quill::telegram::envelope::AuthorizationState::Ready
    ));
    assert_eq!(session.ordered_chats().len(), 1);
    assert_eq!(session.ordered_chats()[0].title, "Alice");
    session.open_chat(quill::ids::ChatId(7));
    let history_extra = session.request(RequestPurpose::GetHistory, Some(quill::ids::ChatId(7)));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"messages","@extra":"{}","messages":[{{"id":50,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hello","entities":[]}}}}}}]}}"#,
            history_extra.0
        )],
    );
    assert!(
        session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .contains_key(&50)
    );
    let send_extra = session.request(RequestPurpose::SendMessage, Some(quill::ids::ChatId(7)));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(
                r#"{{"@type":"message","@extra":"{}","id":-9,"chat_id":7,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_REPLAY_send","entities":[]}}}}}}"#,
                send_extra.0
            ),
            r#"{"@type":"updateMessageSendSucceeded","old_message_id":-9,"message":{"id":51,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_REPLAY_send","entities":[]}}}}"#,
        ],
    );
    let history = session.histories.get(&7).unwrap();
    assert!(!history.messages.contains_key(&-9));
    assert!(history.messages.contains_key(&51));
    assert!(!history.messages.get(&51).unwrap().pending);
    assert!(!sink.rendered().contains("CANARY_REPLAY"));
}

#[test]
fn replay_unread_then_mark_read_and_outbox_receipt() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":2,"last_read_inbox_message_id":40,"last_read_outbox_message_id":0}}"#,
            r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":41,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_UNREAD_one","entities":[]}}}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":42,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_UNREAD_reply","entities":[]}}}}"#,
        ],
    );
    let chat = session.ordered_chats()[0];
    assert_eq!(chat.unread_count, 2);
    // kit Phase 4: the badge label is the kit `Badge`'s `count` (capped at
    // 99, hidden at 0) — no Quill-side label logic left to assert.
    session.open_chat(quill::ids::ChatId(7));
    // Opened with unread messages: nothing is viewed until the UI reports
    // the rows actually on screen (opening must not mark the chat read).
    assert!(
        session
            .message_ids_to_view(quill::ids::ChatId(7))
            .is_empty()
    );
    session.report_visible_messages(
        quill::ids::ChatId(7),
        &[quill::ids::MessageId(41), quill::ids::MessageId(42)],
    );
    let ids = session.message_ids_to_view(quill::ids::ChatId(7));
    assert_eq!(
        ids,
        vec![quill::ids::MessageId(41), quill::ids::MessageId(42)]
    );
    session.mark_viewed(quill::ids::ChatId(7), &ids);
    // Local view does not invent a zero unread count.
    assert_eq!(session.chats.get(&7).unwrap().unread_count, 2);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateChatReadInbox","chat_id":7,"last_read_inbox_message_id":41,"unread_count":0}"#,
            r#"{"@type":"updateChatReadOutbox","chat_id":7,"last_read_outbox_message_id":42}"#,
        ],
    );
    let chat = session.chats.get(&7).unwrap();
    assert_eq!(chat.unread_count, 0);
    let outgoing = session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&42)
        .unwrap();
    assert_eq!(
        chat.outbox_receipt(outgoing),
        quill::state::OutboxReceipt::Read
    );
    assert!(!sink.rendered().contains("CANARY_UNREAD"));
}

#[test]
fn replay_photo_then_update_file_and_document() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":20,"chat_id":7,"is_outgoing":false,"content":{"@type":"messagePhoto","photo":{"@type":"photo","has_stickers":false,"sizes":[{"@type":"photoSize","type":"m","photo":{"@type":"file","id":1,"size":10,"expected_size":10,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}},"width":320,"height":240,"progressive_sizes":[]}]},"caption":{"@type":"formattedText","text":"CANARY_REPLAY_photo","entities":[]},"has_spoiler":false,"is_secret":false}}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":21,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageDocument","document":{"@type":"document","file_name":"notes.txt","mime_type":"text/plain","document":{"@type":"file","id":9,"size":24,"expected_size":24,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}},"caption":{"@type":"formattedText","text":"","entities":[]}}}}"#,
        ],
    );
    session.open_chat(quill::ids::ChatId(7));
    assert_eq!(
        session.thumb_file_ids_to_download(),
        vec![quill::ids::FileId(1)]
    );
    assert!(
        session
            .file(quill::ids::FileId(1))
            .unwrap()
            .needs_download()
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateFile","file":{"@type":"file","id":1,"size":10,"expected_size":10,"local":{"@type":"localFile","path":"/tmp/quill-replay-thumb.jpg","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":true,"download_offset":0,"downloaded_prefix_size":10,"downloaded_size":10},"remote":{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}}}"#,
        ],
    );
    assert_eq!(
        session.file(quill::ids::FileId(1)).unwrap().usable_path(),
        Some("/tmp/quill-replay-thumb.jpg")
    );
    assert!(session.thumb_file_ids_to_download().is_empty());
    let doc = session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&21)
        .unwrap();
    match &doc.content {
        quill::telegram::envelope::MessageContent::Document(d) => {
            assert_eq!(d.file_name, "notes.txt");
            assert_eq!(d.mime_type, "text/plain");
            assert_eq!(d.file_id.0, 9);
        }
        other => panic!("{other:?}"),
    }
    assert!(
        session
            .file(quill::ids::FileId(9))
            .unwrap()
            .needs_download()
    );
    assert!(!sink.rendered().contains("CANARY_REPLAY"));
    assert!(!sink.rendered().contains("CANARY_REMOTE"));
}

#[test]
fn replay_view_messages_error_allows_retry() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":1}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":11,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_VIEW_REPLAY","entities":[]}}}}"#,
        ],
    );
    session.open_chat(quill::ids::ChatId(7));
    let extra = session.request(RequestPurpose::ViewMessages, Some(quill::ids::ChatId(7)));
    session.begin_viewing(quill::ids::ChatId(7), &[quill::ids::MessageId(11)]);
    assert!(
        session
            .message_ids_to_view(quill::ids::ChatId(7))
            .is_empty()
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"error","code":400,"message":"CANARY_VIEW_REPLAY_ERR","@extra":"{}"}}"#,
            extra.0
        )],
    );
    assert!(!session.requests.has_purpose(RequestPurpose::ViewMessages));
    assert_eq!(
        session.message_ids_to_view(quill::ids::ChatId(7)),
        vec![quill::ids::MessageId(11)]
    );
    assert!(!sink.rendered().contains("CANARY_VIEW_REPLAY"));
}

#[test]
fn replay_send_photo_then_document_succeed() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
        ],
    );
    session.open_chat(quill::ids::ChatId(7));
    let photo_extra = session.request(RequestPurpose::SendMessage, Some(quill::ids::ChatId(7)));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(
                r#"{{"@type":"message","@extra":"{}","id":-20,"chat_id":7,"is_outgoing":true,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{{"@type":"file","id":70,"size":10,"expected_size":10,"local":{{"@type":"localFile","path":"","can_be_downloaded":false,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":true,"is_uploading_completed":false,"uploaded_size":0}}}},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"CANARY_REPLAY_out_photo","entities":[]}},"has_spoiler":false,"is_secret":false}}}}"#,
                photo_extra.0
            ),
            r#"{"@type":"updateMessageSendSucceeded","old_message_id":-20,"message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messagePhoto","photo":{"@type":"photo","has_stickers":false,"sizes":[{"@type":"photoSize","type":"m","photo":{"@type":"file","id":70,"size":10,"expected_size":10,"local":{"@type":"localFile","path":"/tmp/quill-out.jpg","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":true,"download_offset":0,"downloaded_prefix_size":10,"downloaded_size":10},"remote":{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}},"width":100,"height":80,"progressive_sizes":[]}]},"caption":{"@type":"formattedText","text":"CANARY_REPLAY_out_photo","entities":[]},"has_spoiler":false,"is_secret":false}}}"#,
        ],
    );
    let photo = session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&60)
        .unwrap();
    assert!(!photo.pending);
    assert!(photo.is_outgoing);
    assert!(matches!(
        photo.content,
        quill::telegram::envelope::MessageContent::Photo(_)
    ));

    let doc_extra = session.request(RequestPurpose::SendMessage, Some(quill::ids::ChatId(7)));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(
                r#"{{"@type":"message","@extra":"{}","id":-21,"chat_id":7,"is_outgoing":true,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"out.txt","mime_type":"text/plain","document":{{"@type":"file","id":71,"size":4,"expected_size":4,"local":{{"@type":"localFile","path":"","can_be_downloaded":false,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":true,"is_uploading_completed":false,"uploaded_size":0}}}}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}"#,
                doc_extra.0
            ),
            r#"{"@type":"updateMessageSendSucceeded","old_message_id":-21,"message":{"id":61,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageDocument","document":{"@type":"document","file_name":"out.txt","mime_type":"text/plain","document":{"@type":"file","id":71,"size":4,"expected_size":4,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":4}}},"caption":{"@type":"formattedText","text":"","entities":[]}}}}"#,
        ],
    );
    let doc = session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&61)
        .unwrap();
    match &doc.content {
        quill::telegram::envelope::MessageContent::Document(d) => {
            assert_eq!(d.file_name, "out.txt");
            assert!(doc.is_outgoing);
            assert!(!doc.pending);
        }
        other => panic!("{other:?}"),
    }
    assert!(!sink.rendered().contains("CANARY_REPLAY"));
    assert!(!sink.rendered().contains("CANARY_REMOTE"));
}

#[test]
fn replay_reply_to_message_quote_and_jump() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":50,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hello already loaded","entities":[]}}}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_REPLAY_reply","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":7,"message_id":50,"quote":null}}}"#,
        ],
    );
    session.open_chat(quill::ids::ChatId(7));
    let reply = session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&60)
        .unwrap();
    assert_eq!(reply.reply_to.as_ref().map(|r| r.message_id.0), Some(50));
    assert_eq!(
        session.reply_quote_preview(reply).as_deref(),
        Some("hello already loaded")
    );
    assert_eq!(
        session.begin_chat_search_jump(quill::ids::MessageId(50)),
        quill::state::ChatSearchJumpNeed::AlreadyReady
    );
    assert_eq!(
        session.search.chat_search.jump,
        quill::state::ChatSearchJump::Ready {
            message_id: quill::ids::MessageId(50)
        }
    );
    assert_eq!(
        session.begin_chat_search_jump(quill::ids::MessageId(40)),
        quill::state::ChatSearchJumpNeed::LoadAround
    );
    let around = session.request_history_around(quill::ids::ChatId(7), quill::ids::MessageId(40));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{{"id":40,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older original","entities":[]}}}}}}]}}"#,
            around.0
        )],
    );
    assert_eq!(
        session.search.chat_search.jump,
        quill::state::ChatSearchJump::Ready {
            message_id: quill::ids::MessageId(40)
        }
    );
    assert!(!sink.rendered().contains("CANARY_REPLAY"));
}

#[test]
fn replay_edit_content_and_delete_tombstone() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"own outgoing","entities":[]}}}}"#,
            r#"{"@type":"updateMessageContent","chat_id":7,"message_id":60,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_REPLAY_edited","entities":[]}}}"#,
        ],
    );
    assert_eq!(
        session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .get(&60)
            .unwrap()
            .content
            .preview(),
        "CANARY_REPLAY_edited"
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateDeleteMessages","chat_id":7,"message_ids":[60],"is_permanent":true,"from_cache":false}"#,
        ],
    );
    let history = session.histories.get(&7).unwrap();
    assert!(!history.contains(quill::ids::MessageId(60)));
    assert!(history.is_tombstone(quill::ids::MessageId(60)));
    assert!(!sink.rendered().contains("CANARY_REPLAY"));
}

#[test]
fn replay_forward_messages_result_and_attribution() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"2","is_pinned":false}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":0}}"#,
            r#"{"@type":"updateChatPosition","chat_id":8,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"1","is_pinned":false}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":50,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hello already here","entities":[]}}}}"#,
        ],
    );
    let extra = session.request(RequestPurpose::ForwardMessages, Some(quill::ids::ChatId(8)));
    session.messages.in_flight_forward = Some(quill::state::ForwardFlight {
        extra,
        dest_chat_id: quill::ids::ChatId(8),
        from_chat_id: quill::ids::ChatId(7),
        requested: 1,
    });
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{{"id":80,"chat_id":8,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hello already here","entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":7}},"date":1}}}}]}}"#,
            extra.0
        )],
    );
    let result = session
        .messages
        .last_forward
        .as_ref()
        .expect("forward result");
    assert_eq!(result.dest_title, "Bob");
    assert_eq!(result.forwarded_ids, vec![quill::ids::MessageId(80)]);
    assert_eq!(result.success_label(), "Forwarded to Bob");
    let dest = session
        .histories
        .get(&8)
        .unwrap()
        .messages
        .get(&80)
        .unwrap();
    assert_eq!(
        session.forward_from_label(dest.forward_info.as_ref().unwrap()),
        "Forwarded from Alice"
    );
    assert!(!sink.rendered().contains("CANARY"));
}

#[test]
fn replay_add_remove_reaction_and_interaction_info() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":50,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hello already here","entities":[]}}}}"#,
        ],
    );
    let extra = session.request(
        RequestPurpose::AddMessageReaction,
        Some(quill::ids::ChatId(7)),
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
            r#"{"@type":"updateMessageInteractionInfo","chat_id":7,"message_id":50,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"total_count":1,"is_chosen":true,"used_sender_id":null,"recent_sender_ids":[]},{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":2,"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}"#,
        ],
    );
    let message = session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&50)
        .unwrap();
    let chips = message.emoji_reaction_chips();
    assert_eq!(chips[0].chip_label().as_deref(), Some("❤ 1"));
    assert!(chips[0].is_chosen);
    assert_eq!(chips[1].chip_label().as_deref(), Some("👍 2"));
    assert!(!chips[1].is_chosen);
    let remove = session.request(
        RequestPurpose::RemoveMessageReaction,
        Some(quill::ids::ChatId(7)),
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, remove.0),
            r#"{"@type":"updateMessageInteractionInfo","chat_id":7,"message_id":50,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":2,"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}"#,
        ],
    );
    let after = session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&50)
        .unwrap();
    assert!(!after.chosen_emoji("❤"));
    assert_eq!(after.emoji_reaction_chips().len(), 1);
    assert!(!sink.rendered().contains("CANARY"));
}

#[test]
fn replay_pin_and_unpin_message_is_pinned() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":50,"chat_id":7,"is_outgoing":false,"is_pinned":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hello already here","entities":[]}}}}"#,
        ],
    );
    session.open_chat = Some(quill::ids::ChatId(7));
    assert!(session.open_chat_pinned_message().is_none());
    let extra = session.request(RequestPurpose::PinChatMessage, Some(quill::ids::ChatId(7)));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
            r#"{"@type":"updateMessageIsPinned","chat_id":7,"message_id":50,"is_pinned":true}"#,
        ],
    );
    let message = session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&50)
        .unwrap();
    assert!(message.is_pinned);
    assert_eq!(session.open_chat_pinned_message().map(|m| m.id.0), Some(50));
    let unpin = session.request(
        RequestPurpose::UnpinChatMessage,
        Some(quill::ids::ChatId(7)),
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, unpin.0),
            r#"{"@type":"updateMessageIsPinned","chat_id":7,"message_id":50,"is_pinned":false}"#,
        ],
    );
    let after = session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&50)
        .unwrap();
    assert!(!after.is_pinned);
    assert!(session.open_chat_pinned_message().is_none());
    assert!(!sink.rendered().contains("CANARY"));
}

#[test]
fn replay_mute_and_archive_move_sidebar_lists() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}}"#,
        ],
    );
    assert_eq!(session.ordered_chats().len(), 1);
    let mute = session.request(
        RequestPurpose::SetChatNotificationSettings,
        Some(quill::ids::ChatId(7)),
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, mute.0),
            r#"{"@type":"updateChatNotificationSettings","chat_id":7,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":false,"mute_for":3600,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":false,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":true,"show_story_poster":false,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}}"#,
        ],
    );
    assert!(session.chats.get(&7).unwrap().is_muted());
    assert_eq!(
        session
            .chats
            .get(&7)
            .unwrap()
            .notification_settings
            .mute_for,
        3600
    );
    let archive = session.request(RequestPurpose::AddChatToList, Some(quill::ids::ChatId(7)));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, archive.0),
            r#"{"@type":"updateChatRemovedFromList","chat_id":7,"chat_list":{"@type":"chatListMain"}}"#,
            r#"{"@type":"updateChatAddedToList","chat_id":7,"chat_list":{"@type":"chatListArchive"}}"#,
            // TDLib moves the row with positions; list membership alone
            // does not (schema 1.8.67, line 3595).
            r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"0","is_pinned":false}}"#,
            r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"4","is_pinned":false}}"#,
        ],
    );
    assert!(session.ordered_chats().is_empty());
    assert!(session.ordered_archived_chats()[0].is_muted());
    assert!(!sink.rendered().contains("CANARY"));
}
