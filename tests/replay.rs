use quill::diagnostics::{Diagnostic, DiagnosticSink, MemorySink};
use quill::ids::AccountKey;
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::{ReceiveBridge, copy_and_parse};
use quill::telegram::envelope::EnvelopePayload;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

fn apply_all(session: &mut Session, sink: &Arc<MemorySink>, jsons: &[&str]) {
    let seq = AtomicU64::new(0);
    apply_all_seq(session, sink, &seq, jsons);
}

fn apply_all_seq(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, jsons: &[&str]) {
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    for json in jsons {
        let owned = copy_and_parse(json, seq, &dyn_sink).unwrap();
        session.apply(owned);
    }
}

#[test]
fn replay_login_to_ready_without_live_network() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    apply_all(
        &mut session,
        &sink,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitTdlibParameters"}}"#,
            r#"{"@type":"ok","@extra":"1"}"#,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}"#,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitCode","code_info":{"@type":"authenticationCodeInfo","type":{"@type":"authenticationCodeTypeSms","length":5}}}}"#,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPassword","password_hint":"CANARY_HINT","has_recovery_email_address":true,"has_passport_data":false,"recovery_email_address_pattern":""}}"#,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        ],
    );
    assert!(matches!(
        session.auth,
        quill::telegram::envelope::AuthorizationState::Ready
    ));
    assert!(!session.auth_view.blocking);
    let logs = sink.rendered();
    assert!(!logs.contains("CANARY_HINT"));
}

#[test]
fn replay_unsupported_auth_halts() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    apply_all(
        &mut session,
        &sink,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPremiumPurchase","store_product_id":"x","premium_day_count":0,"support_email_address":"a@b.c","support_email_subject":"s"}}"#,
        ],
    );
    assert!(matches!(
        session.auth_view.action,
        quill::auth::AuthAction::UnsupportedHalt {
            reason: "premium-purchase"
        }
    ));
}

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
    assert_eq!(
        quill::state::unread_badge_text(chat.unread_count).as_deref(),
        Some("2")
    );
    session.open_chat(quill::ids::ChatId(7));
    let ids = session.message_ids_to_view(quill::ids::ChatId(7));
    assert_eq!(ids.len(), 2);
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
    assert_eq!(quill::state::unread_badge_text(chat.unread_count), None);
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
fn replay_global_search_happy_empty_and_error() {
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
    let recents_gen = session.search.begin_recents();
    let recents_extra =
        session.request_search(RequestPurpose::SearchRecentlyFoundChats, recents_gen);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[7]}}"#,
            recents_extra.0
        )],
    );
    assert_eq!(session.search.status, quill::state::SearchStatus::Ready);
    assert!(session.search.recents);
    assert_eq!(session.search.chat_ids[0].0, 7);

    let search_gen = session.search.begin_query("hello");
    let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
    let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
    let public_extra = session.request_search(RequestPurpose::SearchPublicChats, search_gen);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[7]}}"#,
                chats_extra.0
            ),
            &format!(
                r#"{{"@type":"foundMessages","@extra":"{}","total_count":1,"next_offset":"","messages":[{{"id":50,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_REPLAY_search","entities":[]}}}}}}]}}"#,
                messages_extra.0
            ),
            // Phase 7.2 replay: `searchPublicChats` returns an unknown public
            // channel; status waits for it before resolving to Ready.
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[4242]}}"#,
                public_extra.0
            ),
        ],
    );
    assert_eq!(session.search.status, quill::state::SearchStatus::Ready);
    assert_eq!(session.search.chat_ids[0].0, 7);
    assert_eq!(session.search.public_chat_ids[0].0, 4242);
    session.promote_search_message(quill::ids::ChatId(7), quill::ids::MessageId(50));
    session.open_chat(quill::ids::ChatId(7));
    assert!(
        session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .contains_key(&50)
    );

    let search_gen = session.search.begin_query("zzz");
    let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
    let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
    let public_extra = session.request_search(RequestPurpose::SearchPublicChats, search_gen);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
                chats_extra.0
            ),
            &format!(
                r#"{{"@type":"foundMessages","@extra":"{}","total_count":0,"next_offset":"","messages":[]}}"#,
                messages_extra.0
            ),
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
                public_extra.0
            ),
        ],
    );
    assert_eq!(session.search.status, quill::state::SearchStatus::Empty);

    let search_gen = session.search.begin_query("nope");
    let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
    let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
    let public_extra = session.request_search(RequestPurpose::SearchPublicChats, search_gen);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_REPLAY_search_err","@extra":"{}"}}"#,
                chats_extra.0
            ),
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_REPLAY_search_err2","@extra":"{}"}}"#,
                messages_extra.0
            ),
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_REPLAY_search_err3","@extra":"{}"}}"#,
                public_extra.0
            ),
        ],
    );
    assert_eq!(session.search.status, quill::state::SearchStatus::Failed);
    session.close_search();
    assert_eq!(session.search.status, quill::state::SearchStatus::Closed);
    assert!(!sink.rendered().contains("CANARY_REPLAY"));
}

#[test]
fn replay_chat_search_generation_jump_and_empty() {
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
            r#"{"@type":"updateNewMessage","message":{"id":50,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hello already loaded","entities":[]}}}}"#,
        ],
    );
    session.open_chat(quill::ids::ChatId(7));
    assert!(session.open_chat_search());
    let search_gen = session.chat_search.begin_query("hello");
    let extra = session.request_chat_search(
        RequestPurpose::SearchChatMessages,
        quill::ids::ChatId(7),
        search_gen,
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":40,"messages":[{{"id":50,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_REPLAY_chat","entities":[]}}}}}},{{"id":40,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older hello","entities":[]}}}}}}]}}"#,
            extra.0
        )],
    );
    assert_eq!(
        session.chat_search.status,
        quill::state::SearchStatus::Ready
    );
    assert_eq!(session.chat_search.hits.len(), 2);
    assert_eq!(
        session.begin_chat_search_jump(quill::ids::MessageId(50)),
        quill::state::ChatSearchJumpNeed::AlreadyReady
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
            r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{{"id":40,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older hello","entities":[]}}}}}},{{"id":39,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"neighbor","entities":[]}}}}}}]}}"#,
            around.0
        )],
    );
    assert_eq!(
        session.chat_search.jump,
        quill::state::ChatSearchJump::Ready {
            message_id: quill::ids::MessageId(40)
        }
    );
    assert!(
        session
            .histories
            .get(&7)
            .unwrap()
            .contains(quill::ids::MessageId(39))
    );

    let stale = session.chat_search.begin_query("old");
    let stale_extra = session.request_chat_search(
        RequestPurpose::SearchChatMessages,
        quill::ids::ChatId(7),
        stale,
    );
    let fresh = session.chat_search.begin_query("new");
    let _fresh_extra = session.request_chat_search(
        RequestPurpose::SearchChatMessages,
        quill::ids::ChatId(7),
        fresh,
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[{{"id":1,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"stale","entities":[]}}}}}}]}}"#,
            stale_extra.0
        )],
    );
    assert_eq!(
        session.chat_search.status,
        quill::state::SearchStatus::Searching
    );
    assert!(session.chat_search.hits.is_empty());

    let empty_gen = session.chat_search.begin_query("zzz");
    let empty_extra = session.request_chat_search(
        RequestPurpose::SearchChatMessages,
        quill::ids::ChatId(7),
        empty_gen,
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":0,"next_from_message_id":0,"messages":[]}}"#,
            empty_extra.0
        )],
    );
    assert_eq!(
        session.chat_search.status,
        quill::state::SearchStatus::Empty
    );

    let open_chat = session.open_chat;
    session.close_chat_search();
    assert_eq!(
        session.chat_search.status,
        quill::state::SearchStatus::Closed
    );
    assert_eq!(session.open_chat, open_chat);
    assert!(
        session
            .histories
            .get(&7)
            .unwrap()
            .contains(quill::ids::MessageId(50))
    );
    assert!(!sink.rendered().contains("CANARY_REPLAY"));
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
        session.chat_search.jump,
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
        session.chat_search.jump,
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
    session.in_flight_forward = Some(quill::state::ForwardFlight {
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
    let result = session.last_forward.as_ref().expect("forward result");
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
            r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"4","is_pinned":false}}"#,
        ],
    );
    assert!(session.ordered_chats().is_empty());
    assert!(session.ordered_archived_chats()[0].is_muted());
    assert!(!sink.rendered().contains("CANARY"));
}

#[test]
fn logout_invalidates_pending_requests() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let _ = session.request(RequestPurpose::GetHistory, Some(quill::ids::ChatId(1)));
    assert_eq!(session.requests.len(), 1);
    session.begin_logout();
    assert_eq!(session.requests.len(), 0);
}

#[test]
fn ordered_bridge_rejects_payload_logging() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let bridge = ReceiveBridge::spawn_injected(dyn_sink);
    bridge.inject(r#"{"@type":"error","code":400,"message":"CANARY_PHONE_+1999","@extra":"3"}"#);
    let env = bridge.next_timeout(Duration::from_secs(1)).unwrap();
    assert!(matches!(env.envelope.payload, EnvelopePayload::Error(_)));
    let rendered = sink.rendered();
    assert!(!rendered.contains("CANARY_PHONE"));
    assert!(!rendered.contains("+1999"));
}

#[test]
fn diagnostics_never_include_message_text() {
    let sink = MemorySink::new();
    sink.record(Diagnostic {
        category: "td-receive",
        type_name: Some("updateNewMessage".into()),
        extra: Some(4),
        seq: Some(1),
        note: "ok",
    });
    assert!(!sink.rendered().contains("hello secret"));
}
/// Phase 2.2 channel fixtures. The channel chat is now ungated: sponsored rows
/// fetch through the same pipeline while history opens normally.
fn sponsored_test_session(sink: &Arc<MemorySink>, seq: &AtomicU64) -> Session {
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    apply_all_seq(
        &mut session,
        sink,
        seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
        ],
    );
    // Channels are supported since Phase 2.2; the sponsored pipeline runs for
    // the open channel.
    let chat = session.chats.get(&13).unwrap();
    assert!(chat.supported());
    assert!(chat.kind.gate_reason().is_none());
    assert!(chat.is_channel());
    assert!(!chat.can_post());

    session.open_chat(quill::ids::ChatId(13));
    let extra = session.request(
        RequestPurpose::GetChatSponsoredMessages,
        Some(quill::ids::ChatId(13)),
    );
    let thumb = r#"{"@type":"file","id":61,"size":24,"expected_size":24,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}"#;
    apply_all_seq(
        &mut session,
        sink,
        seq,
        &[&format!(
            r#"{{"@type":"sponsoredMessages","@extra":"{}","messages_between":3,"messages":[{{"@type":"sponsoredMessage","message_id":9001,"is_recommended":false,"can_be_reported":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"summer sale","entities":[]}}}},"sponsor":{{"@type":"advertisementSponsor","url":"https://example.com/promo","photo":{{"@type":"photo","has_stickers":false,"sizes":[]}},"info":"Example Ads"}},"title":"Summer sale","button_text":"Shop now","accent_color_id":0,"background_custom_emoji_id":"0","additional_info":"Ad by Example"}},{{"@type":"sponsoredMessage","message_id":9002,"is_recommended":true,"can_be_reported":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"pick","entities":[]}},"has_spoiler":false,"is_secret":false}},"sponsor":{{"@type":"advertisementSponsor","url":"https://example.com/pick","photo":{{"@type":"photo","has_stickers":false,"sizes":[]}},"info":"Curated"}},"title":"Editors' pick","button_text":"Learn more","accent_color_id":0,"background_custom_emoji_id":"0","additional_info":""}}]}}"#,
            extra.0
        )],
    );
    session
}

#[test]
fn replay_sponsored_messages_fetch_and_labels() {
    let sink = Arc::new(MemorySink::new());
    let seq = AtomicU64::new(0);
    let session = sponsored_test_session(&sink, &seq);

    let entry = session.sponsored.get(&13).unwrap();
    assert_eq!(entry.messages_between, 3);
    assert_eq!(entry.messages.len(), 2);
    assert_eq!(entry.messages[0].kind_label(), "Sponsored");
    assert_eq!(entry.messages[1].kind_label(), "Recommended");
    assert!(entry.messages[0].can_be_reported);
    assert!(!entry.messages[1].can_be_reported);
    assert_eq!(entry.messages[0].title, "Summer sale");
    assert_eq!(entry.messages[0].sponsor.url, "https://example.com/promo");
    assert_eq!(entry.messages[0].sponsor.info, "Example Ads");
    assert_eq!(entry.messages[0].button_text, "Shop now");
    assert_eq!(entry.messages[0].additional_info, "Ad by Example");

    // Sponsored thumbs join the priority-1 download pass for the open chat.
    let thumbs = session.thumb_file_ids_to_download();
    assert!(thumbs.iter().any(|id| id.0 == 61));

    // Rows preserve the TDLib response vector order (no sort applied).
    let rows = session.open_sponsored_rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].message_id, 9001);
    assert_eq!(rows[1].message_id, 9002);
}

#[test]
fn replay_sponsored_report_option_required_then_ok() {
    let sink = Arc::new(MemorySink::new());
    let seq = AtomicU64::new(0);
    let mut session = sponsored_test_session(&sink, &seq);
    let chat_id = quill::ids::ChatId(13);

    // The Recommended row is not reportable.
    assert!(session.begin_sponsored_report(chat_id, 9002).is_none());
    assert!(session.sponsored_report.is_none());

    assert!(session.begin_sponsored_report(chat_id, 9001).is_some());
    let extra = session.request(RequestPurpose::ReportChatSponsoredMessage, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"reportSponsoredResultOptionRequired","@extra":"{}","title":"Why report?","options":[{{"@type":"reportOption","id":"bWlzLWxlYWQ=","text":"Misleading"}},{{"@type":"reportOption","id":"c3BhbQ==","text":"Spam"}}]}}"#,
            extra.0
        )],
    );
    let flight = session.sponsored_report.clone().unwrap();
    assert_eq!(flight.chat_id, chat_id);
    assert_eq!(flight.message_id, 9001);
    assert_eq!(flight.title, "Why report?");
    assert_eq!(flight.options.len(), 2);
    assert_eq!(flight.options[0].text, "Misleading");
    assert_eq!(flight.options[1].text, "Spam");

    // Follow-up with the chosen option id.
    assert!(session.begin_sponsored_report(chat_id, 9001).is_some());
    let extra = session.request(RequestPurpose::ReportChatSponsoredMessage, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"reportSponsoredResultOk","@extra":"{}"}}"#,
            extra.0
        )],
    );
    assert!(session.sponsored_report.is_none());
    let outcome = session.last_sponsored_report.clone().unwrap();
    assert_eq!(outcome.chat_id, chat_id);
    assert_eq!(outcome.message_id, 9001);
    assert_eq!(outcome.user_message(), "Report sent");
}

#[test]
fn replay_sponsored_report_failed() {
    let sink = Arc::new(MemorySink::new());
    let seq = AtomicU64::new(0);
    let mut session = sponsored_test_session(&sink, &seq);
    let chat_id = quill::ids::ChatId(13);

    assert!(session.begin_sponsored_report(chat_id, 9001).is_some());
    let extra = session.request(RequestPurpose::ReportChatSponsoredMessage, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"reportSponsoredResultFailed","@extra":"{}"}}"#,
            extra.0
        )],
    );
    assert!(session.sponsored_report.is_none());
    let outcome = session.last_sponsored_report.clone().unwrap();
    assert_eq!(outcome.user_message(), "Could not report this message");
}

#[test]
fn replay_sponsored_report_ads_hidden_and_premium_required() {
    let sink = Arc::new(MemorySink::new());
    let seq = AtomicU64::new(0);
    let mut session = sponsored_test_session(&sink, &seq);
    let chat_id = quill::ids::ChatId(13);

    for (ctor, message) in [
        (
            "reportSponsoredResultAdsHidden",
            "Sponsored messages hidden",
        ),
        (
            "reportSponsoredResultPremiumRequired",
            "Hiding sponsored messages needs Telegram Premium",
        ),
    ] {
        assert!(session.begin_sponsored_report(chat_id, 9001).is_some());
        let extra = session.request(RequestPurpose::ReportChatSponsoredMessage, Some(chat_id));
        apply_all_seq(
            &mut session,
            &sink,
            &seq,
            &[&format!(r#"{{"@type":"{}","@extra":"{}"}}"#, ctor, extra.0)],
        );
        let outcome = session.last_sponsored_report.clone().unwrap();
        assert_eq!(outcome.user_message(), message);
    }
}

/// Phase 2.2: broadcast channels appear in the chat list with their title
/// (ungated) and no gate-reason preview.
#[test]
fn replay_channel_ungated_in_chat_list() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
            r#"{"@type":"updateChatPosition","chat_id":13,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"10","is_pinned":false}}"#,
        ],
    );
    let ids: Vec<i64> = session.ordered_chats().iter().map(|c| c.id.0).collect();
    assert_eq!(ids, vec![13]);
    let chat = session.chats.get(&13).unwrap();
    assert!(chat.supported());
    assert!(chat.is_channel());
    assert!(chat.kind.gate_reason().is_none());
    assert_eq!(chat.title, "Demo channel");
    assert_eq!(chat.sidebar_preview(), "cloud chat");
}

/// Phase 2.2: broadcast posts render with the channel as author and live
/// `interaction_info.view_count` (`updateMessageInteractionInfo` included).
#[test]
fn replay_broadcast_posts_with_view_counts() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":201,"chat_id":13,"sender_id":{"@type":"messageSenderChat","chat_id":13},"is_outgoing":false,"is_channel_post":true,"interaction_info":{"@type":"messageInteractionInfo","view_count":12345,"forward_count":7,"reply_info":null,"reactions":null},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"post one","entities":[]}}}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":202,"chat_id":13,"sender_id":{"@type":"messageSenderChat","chat_id":13},"is_outgoing":false,"is_channel_post":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"no views yet","entities":[]}}}}"#,
        ],
    );
    let history = session.histories.get(&13).unwrap();
    assert_eq!(history.messages.len(), 2);
    let first = &history.messages[&201];
    assert!(!first.is_outgoing);
    assert_eq!(
        first.interaction_info.as_ref().map(|info| info.view_count),
        Some(12345)
    );
    let second = &history.messages[&202];
    assert!(second.interaction_info.is_none());
    // Live bump via `updateMessageInteractionInfo`.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateMessageInteractionInfo","chat_id":13,"message_id":201,"interaction_info":{"@type":"messageInteractionInfo","view_count":12402,"forward_count":7,"reply_info":null,"reactions":null}}"#,
        ],
    );
    let first = &session.histories.get(&13).unwrap().messages[&201];
    assert_eq!(
        first.interaction_info.as_ref().map(|info| info.view_count),
        Some(12402)
    );
}

/// Phase 2.2/2.3: own membership flows through `getMe` / `getChatMember` /
/// `joinChat` / `updateChatMember` / `leaveChat`; the composer stays hidden
/// for non-admins and appears for admins (2.3).
#[test]
fn replay_channel_membership_and_join_leave() {
    use quill::telegram::envelope::ChannelMemberStatus;

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let chat_id = quill::ids::ChatId(13);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
        ],
    );
    let chat = session.chats.get(&13).unwrap();
    // Composer hidden in channels (admin posting is 2.3).
    assert!(!chat.can_post());
    assert_eq!(chat.my_member_status, None);

    let me_extra = session.request(RequestPurpose::GetMe, None);
    let member_extra = session.request(RequestPurpose::GetChatMember, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(r#"{{"@type":"user","@extra":"{}","id":777}}"#, me_extra.0),
            &format!(
                r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{{"@type":"chatMemberStatusLeft"}}}}"#,
                member_extra.0
            ),
        ],
    );
    assert_eq!(session.my_user_id, Some(777));
    let chat = session.chats.get(&13).unwrap();
    assert_eq!(chat.my_member_status, Some(ChannelMemberStatus::Left));

    // joinChat → success flips to Member optimistically.
    let join_extra = session.request(RequestPurpose::JoinChat, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"chatJoinResultSuccess","@extra":"{}","chat_id":13}}"#,
            join_extra.0
        )],
    );
    assert_eq!(
        session.chats.get(&13).unwrap().my_member_status,
        Some(ChannelMemberStatus::Member)
    );

    // updateChatMember confirms the admin promotion; in 2.3 a bare admin
    // (no rights block → no explicit restriction) gets the composer.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateChatMember","chat_id":13,"actor_user_id":1,"date":1,"invite_link":null,"via_join_request":false,"via_chat_folder_invite_link":false,"old_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":777},"status":{"@type":"chatMemberStatusMember"}},"new_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":777},"status":{"@type":"chatMemberStatusAdministrator"}}}"#,
        ],
    );
    let chat = session.chats.get(&13).unwrap();
    assert_eq!(
        chat.my_member_status,
        Some(ChannelMemberStatus::Administrator)
    );
    assert!(chat.my_member_status.unwrap().is_admin());
    assert!(chat.can_post());

    // leaveChat → ok flips to Left optimistically.
    let leave_extra = session.request(RequestPurpose::LeaveChat, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(r#"{{"@type":"ok","@extra":"{}"}}"#, leave_extra.0)],
    );
    assert_eq!(
        session.chats.get(&13).unwrap().my_member_status,
        Some(ChannelMemberStatus::Left)
    );
}

/// Phase 2.2: non-success `joinChat` results keep the old status.
#[test]
fn replay_join_chat_non_success_keeps_status() {
    use quill::telegram::envelope::ChannelMemberStatus;

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let chat_id = quill::ids::ChatId(13);
    session.my_user_id = Some(777);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
        ],
    );
    let chat = session.chats.get_mut(&13).unwrap();
    chat.set_member_status(ChannelMemberStatus::Left, None);
    for ctor in [
        "chatJoinResultRequestSent",
        "chatJoinResultGuardBotApprovalRequired",
        "chatJoinResultDeclined",
    ] {
        let extra = session.request(RequestPurpose::JoinChat, Some(chat_id));
        apply_all_seq(
            &mut session,
            &sink,
            &seq,
            &[&format!(r#"{{"@type":"{}","@extra":"{}"}}"#, ctor, extra.0)],
        );
        assert_eq!(
            session.chats.get(&13).unwrap().my_member_status,
            Some(ChannelMemberStatus::Left),
            "{ctor} must not flip status"
        );
    }
    // A foreign member update is ignored.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateChatMember","chat_id":13,"actor_user_id":1,"date":1,"invite_link":null,"via_join_request":false,"via_chat_folder_invite_link":false,"old_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":999},"status":{"@type":"chatMemberStatusMember"}},"new_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":999},"status":{"@type":"chatMemberStatusBanned"}}}"#,
        ],
    );
    assert_eq!(
        session.chats.get(&13).unwrap().my_member_status,
        Some(ChannelMemberStatus::Left)
    );
}

/// Phase 2.3: an administrator with `rights.can_post_messages: true` gets the
/// composer (`ChatSummary::can_post()` true — the exact predicate the
/// composer gate in `src/ui/mod.rs` reads).
#[test]
fn replay_channel_admin_sees_composer() {
    use quill::telegram::envelope::ChannelMemberStatus;

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let chat_id = quill::ids::ChatId(13);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
        ],
    );
    // No membership yet: composer stays hidden.
    assert!(!session.chats.get(&13).unwrap().can_post());

    let me_extra = session.request(RequestPurpose::GetMe, None);
    let member_extra = session.request(RequestPurpose::GetChatMember, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(r#"{{"@type":"user","@extra":"{}","id":777}}"#, me_extra.0),
            &format!(
                r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{{"@type":"chatAdministratorRights","can_post_messages":true}}}}}}"#,
                member_extra.0
            ),
        ],
    );
    let chat = session.chats.get(&13).unwrap();
    assert_eq!(
        chat.my_member_status,
        Some(ChannelMemberStatus::Administrator)
    );
    assert_eq!(chat.my_admin_can_post_messages, Some(true));
    assert!(chat.can_post());

    // A channel post sent through the normal pipeline still renders.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":13,"sender_id":{"@type":"messageSenderChat","chat_id":13},"is_outgoing":false,"is_channel_post":true,"interaction_info":{"@type":"messageInteractionInfo","view_count":5,"forward_count":0,"reply_info":null,"reactions":null},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"admin post echo","entities":[]}}}}"#,
        ],
    );
    assert!(
        session
            .histories
            .get(&13)
            .unwrap()
            .messages
            .contains_key(&301)
    );
}

/// Phase 2.3: non-admins keep the hidden composer; so does an administrator
/// whose `rights.can_post_messages` is explicitly false.
#[test]
fn replay_channel_non_admin_composer_hidden() {
    use quill::telegram::envelope::ChannelMemberStatus;

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let chat_id = quill::ids::ChatId(13);
    session.my_user_id = Some(777);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
        ],
    );

    // Plain member: composer hidden.
    let member_extra = session.request(RequestPurpose::GetChatMember, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{{"@type":"chatMemberStatusMember"}}}}"#,
            member_extra.0
        )],
    );
    let chat = session.chats.get(&13).unwrap();
    assert_eq!(chat.my_member_status, Some(ChannelMemberStatus::Member));
    assert!(!chat.can_post());

    // Administrator with the posting right revoked: composer hidden too.
    let member_extra = session.request(RequestPurpose::GetChatMember, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{{"@type":"chatAdministratorRights","can_post_messages":false}}}}}}"#,
            member_extra.0
        )],
    );
    let chat = session.chats.get(&13).unwrap();
    assert_eq!(
        chat.my_member_status,
        Some(ChannelMemberStatus::Administrator)
    );
    assert_eq!(chat.my_admin_can_post_messages, Some(false));
    assert!(!chat.can_post());
}

/// Phase 2.3: `updateChatMember` flips the composer gate both ways —
/// member → admin shows it, admin → left hides it, creator shows it.
#[test]
fn replay_channel_admin_status_change_flips_composer() {
    use quill::telegram::envelope::ChannelMemberStatus;

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    session.my_user_id = Some(777);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
        ],
    );
    let chat = session.chats.get_mut(&13).unwrap();
    chat.set_member_status(ChannelMemberStatus::Member, None);
    assert!(!session.chats.get(&13).unwrap().can_post());

    let promote = |status_json: &str| {
        format!(
            r#"{{"@type":"updateChatMember","chat_id":13,"actor_user_id":1,"date":1,"invite_link":null,"via_join_request":false,"via_chat_folder_invite_link":false,"old_chat_member":{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{{"@type":"chatMemberStatusMember"}}}},"new_chat_member":{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{}}}}}"#,
            status_json
        )
    };
    let admin_rights = |can_post: bool| {
        format!(
            r#"{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{{"@type":"chatAdministratorRights","can_post_messages":{can_post}}}}}"#
        )
    };

    // Promoted to admin with the posting right: composer appears.
    apply_all_seq(&mut session, &sink, &seq, &[&promote(&admin_rights(true))]);
    let chat = session.chats.get(&13).unwrap();
    assert_eq!(
        chat.my_member_status,
        Some(ChannelMemberStatus::Administrator)
    );
    assert!(chat.can_post());

    // Right revoked server-side: composer hides again.
    apply_all_seq(&mut session, &sink, &seq, &[&promote(&admin_rights(false))]);
    let chat = session.chats.get(&13).unwrap();
    assert_eq!(chat.my_admin_can_post_messages, Some(false));
    assert!(!chat.can_post());

    // Left the channel: still hidden.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&promote(r#"{"@type":"chatMemberStatusLeft"}"#)],
    );
    assert_eq!(
        session.chats.get(&13).unwrap().my_member_status,
        Some(ChannelMemberStatus::Left)
    );
    assert!(!session.chats.get(&13).unwrap().can_post());

    // Became the creator: composer appears.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&promote(r#"{"@type":"chatMemberStatusCreator"}"#)],
    );
    let chat = session.chats.get(&13).unwrap();
    assert_eq!(chat.my_member_status, Some(ChannelMemberStatus::Creator));
    assert!(chat.can_post());
}

const BOT_USER_JSON: &str = r#"{"@type":"updateUser","user":{"id":21,"first_name":"Demo","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#;

/// Phase 3.1: bot private chats ride the ordinary private-chat path — they
/// are listed, ungated, open, render history, and keep the composer. The
/// gating infrastructure (`is_supported_cloud_chat` / `gate_reason` /
/// `can_post`) is untouched.
#[test]
fn replay_bot_chat_ungated_with_history() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            BOT_USER_JSON,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
            r#"{"@type":"updateChatPosition","chat_id":21,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"40","is_pinned":false}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"ping","entities":[]}}}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":302,"chat_id":21,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"/start","entities":[]}}}}"#,
        ],
    );
    // Chat list: the bot chat appears like any private chat.
    let ids: Vec<i64> = session.ordered_chats().iter().map(|c| c.id.0).collect();
    assert_eq!(ids, vec![21]);
    let chat = session.chats.get(&21).unwrap();
    assert!(chat.supported());
    assert!(chat.kind.gate_reason().is_none());
    assert!(chat.can_post());
    // Bot detection feeds the lazy `getUserFullInfo` fetch.
    assert_eq!(
        session.bot_user_id_for_chat(quill::ids::ChatId(21)),
        Some(21)
    );
    // History renders bot and own messages.
    let history = session.histories.get(&21).unwrap();
    assert_eq!(history.messages.len(), 2);
    assert!(!history.messages[&301].is_outgoing);
    assert!(history.messages[&302].is_outgoing);
}

/// Phase 3.1: a `getUserFullInfo` response caches `botInfo` (description +
/// commands) for the bot chat; the panel reads it back.
#[test]
fn replay_bot_info_cached_from_full_info() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            BOT_USER_JSON,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
        ],
    );
    assert!(session.bot_info_for_chat(quill::ids::ChatId(21)).is_none());
    let extra = session.request(
        RequestPurpose::GetUserFullInfo,
        Some(quill::ids::ChatId(21)),
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"A demo bot","description":"CANARY_desc","commands":[{{"@type":"botCommand","command":"start","description":"Start the bot","is_ephemeral":false}},{{"@type":"botCommand","command":"help","description":"Show help","is_ephemeral":false}}]}}}}"#,
            extra.0
        )],
    );
    let info = session
        .bot_info_for_chat(quill::ids::ChatId(21))
        .expect("bot info cached");
    assert_eq!(info.short_description, "A demo bot");
    assert_eq!(info.description, "CANARY_desc");
    assert_eq!(info.commands.len(), 2);
    assert_eq!(info.commands[0].command, "start");
    assert_eq!(info.commands[0].description, "Start the bot");
    assert_eq!(info.commands[1].command, "help");
}

/// Phase 3.3: a `getCommands` response caches the global-scope commands and
/// merges them below the `botInfo` commands in `command_menu_items`
/// (deduped by command name); a stray `botCommands` with no matching
/// pending request is ignored.
#[test]
fn replay_bot_commands_cached_from_get_commands() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            BOT_USER_JSON,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
        ],
    );
    let chat = quill::ids::ChatId(21);
    assert!(session.command_menu_items(chat).is_empty());

    // `botInfo` commands first (specific).
    let full_extra = session.request(RequestPurpose::GetUserFullInfo, Some(chat));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"","description":"","commands":[{{"@type":"botCommand","command":"start","description":"Start the bot","is_ephemeral":false}}]}}}}"#,
            full_extra.0
        )],
    );

    // `botCommands` response to the matching `getCommands` request
    // (global scope).
    let cmd_extra = session.request(RequestPurpose::GetCommands, Some(chat));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"botCommands","@extra":"{}","bot_user_id":21,"commands":[{{"@type":"botCommand","command":"settings","description":"Global settings","is_ephemeral":false}},{{"@type":"botCommand","command":"start","description":"Global start","is_ephemeral":false}}]}}"#,
            cmd_extra.0
        )],
    );
    let items = session.command_menu_items(chat);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].command, "start");
    assert_eq!(items[0].description, "Start the bot");
    assert!(!items[0].global);
    // `settings` is new and global; the `start` duplicate is dropped so
    // the bot-specific description wins.
    assert_eq!(items[1].command, "settings");
    assert_eq!(items[1].description, "Global settings");
    assert!(items[1].global);

    // A stray `botCommands` with no matching pending request is ignored.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"botCommands","@extra":"4242","bot_user_id":21,"commands":[{"@type":"botCommand","command":"evil","description":"","is_ephemeral":false}]}"#,
        ],
    );
    let items = session.command_menu_items(chat);
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|item| item.command != "evil"));
}

/// Phase 3.3: an `error` answer to `getCommands` records an empty command
/// set — the menu falls back to the `botInfo` commands and the driver
/// will not retry the fetch.
#[test]
fn replay_bot_commands_error_records_empty_set() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            BOT_USER_JSON,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
        ],
    );
    let chat = quill::ids::ChatId(21);
    let cmd_extra = session.request(RequestPurpose::GetCommands, Some(chat));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CANARY_bots_only"}}"#,
            cmd_extra.0
        )],
    );
    assert!(session.bot_commands.contains_key(&21));
    assert!(session.command_menu_items(chat).is_empty());
}

/// Phase 3.1: `updateUserFullInfo` refreshes the cached bot info live.
#[test]
fn replay_bot_info_refreshed_by_update() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            BOT_USER_JSON,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
            r#"{"@type":"updateUserFullInfo","user_id":21,"user_full_info":{"@type":"userFullInfo","bot_info":{"@type":"botInfo","short_description":"","description":"CANARY_refreshed","commands":[{"@type":"botCommand","command":"ping","description":"","is_ephemeral":false}]}}}"#,
        ],
    );
    let info = session
        .bot_info_for_chat(quill::ids::ChatId(21))
        .expect("bot info cached");
    assert_eq!(info.description, "CANARY_refreshed");
    assert_eq!(info.commands.len(), 1);
    assert_eq!(info.commands[0].command, "ping");
    // If the user stops being a bot, the stale bot info is dropped.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateUser","user":{"id":21,"first_name":"Demo","type":{"@type":"userTypeRegular"}}}"#,
        ],
    );
    assert!(session.bot_info_for_chat(quill::ids::ChatId(21)).is_none());
    assert_eq!(session.bot_user_id_for_chat(quill::ids::ChatId(21)), None);
}

/// Phase B1: secret chats are supported, with posting gated on the
/// lifecycle state (schema 1.8.67 lines 2798–2804). `updateSecretChat`
/// arrives before `updateNewChat` (line 10740): a Ready chat posts, while
/// Pending and Closed chats do not. Gating for everything else is
/// unchanged: a non-bot private chat is not a bot chat (no bot-info
/// lookup, no draft change).
#[test]
fn replay_secret_chat_lifecycle_states() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            // `updateSecretChat` first, per the schema ordering guarantee.
            r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":31,"user_id":7,"state":{"@type":"secretChatStateReady"},"is_outbound":true,"key_hash":"","layer":144}}"#,
            r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":33,"user_id":7,"state":{"@type":"secretChatStatePending"},"is_outbound":true,"key_hash":"","layer":144}}"#,
            r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":34,"user_id":7,"state":{"@type":"secretChatStateClosed"},"is_outbound":false,"key_hash":"","layer":144}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Secret","type":{"@type":"chatTypeSecret","secret_chat_id":31,"user_id":7},"unread_count":0}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":33,"title":"Secret pending","type":{"@type":"chatTypeSecret","secret_chat_id":33,"user_id":7},"unread_count":0}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":34,"title":"Secret closed","type":{"@type":"chatTypeSecret","secret_chat_id":34,"user_id":7},"unread_count":0}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":32,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
        ],
    );
    let secret = session.chats.get(&31).unwrap();
    assert!(secret.supported());
    assert!(secret.kind.gate_reason().is_none());
    assert!(secret.can_post());
    let pending = session.chats.get(&33).unwrap();
    assert!(pending.supported());
    assert!(!pending.can_post());
    let closed = session.chats.get(&34).unwrap();
    assert!(closed.supported());
    assert!(!closed.can_post());
    let peer = session.chats.get(&32).unwrap();
    assert!(peer.supported());
    assert!(peer.kind.gate_reason().is_none());
    assert!(peer.can_post());
    assert_eq!(session.bot_user_id_for_chat(quill::ids::ChatId(32)), None);
    assert!(session.bot_info_for_chat(quill::ids::ChatId(32)).is_none());
}

/// Phase B1: a secret chat whose state has never been seen (e.g. loaded
/// from the local DB with no `updateSecretChat` yet) queues an offline
/// `getSecretChat` fetch instead of guessing — the driver drains the
/// queue in `ingest`.
#[test]
fn replay_secret_chat_unknown_state_queues_get_secret_chat() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Secret","type":{"@type":"chatTypeSecret","secret_chat_id":31,"user_id":7},"unread_count":0}}"#,
        ],
    );
    let secret = session.chats.get(&31).unwrap();
    assert!(secret.supported());
    assert!(!secret.can_post());
    assert!(session.secret_chat_fetch_queue.contains(&31));
    // No duplicate queueing if the same chat arrives again.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Secret","type":{"@type":"chatTypeSecret","secret_chat_id":31,"user_id":7},"unread_count":0}}"#,
        ],
    );
    assert_eq!(
        session
            .secret_chat_fetch_queue
            .iter()
            .filter(|id| **id == 31)
            .count(),
        1
    );
}

/// Phase B1: the secret-chat state transitions Pending → Ready → Closed,
/// each `updateSecretChat` fanning out to the chat summary.
#[test]
fn replay_secret_chat_pending_to_ready_to_closed() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":31,"user_id":7,"state":{"@type":"secretChatStatePending"},"is_outbound":true,"key_hash":"","layer":144}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Secret","type":{"@type":"chatTypeSecret","secret_chat_id":31,"user_id":7},"unread_count":0}}"#,
        ],
    );
    let chat = session.chats.get(&31).unwrap();
    assert!(!chat.can_post());
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":31,"user_id":7,"state":{"@type":"secretChatStateReady"},"is_outbound":true,"key_hash":"","layer":144}}"#,
        ],
    );
    let chat = session.chats.get(&31).unwrap();
    assert!(chat.can_post());
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":31,"user_id":7,"state":{"@type":"secretChatStateClosed"},"is_outbound":true,"key_hash":"","layer":144}}"#,
        ],
    );
    let chat = session.chats.get(&31).unwrap();
    assert!(!chat.can_post());
    assert!(session.secret_chat_fetch_queue.is_empty());
}

/// Phase 3.2: an `updateNewMessage` with real `replyMarkupInlineKeyboard`
/// JSON stores the keyboard with the message. URL buttons carry an
/// openable URL (the dispatch predicate — the browser itself is not opened
/// in tests); callback buttons carry the payload bytes; switchInline buttons
/// carry the query; unknown button types are kept as disabled placeholders.
#[test]
fn replay_inline_keyboard_stored_from_real_json() {
    use quill::telegram::envelope::{InlineKeyboardButtonType, InlineKeyboardTargetChat};
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Visit site","icon_custom_emoji_id":0,"style":{"@type":"buttonStylePrimary"},"type":{"@type":"inlineKeyboardButtonTypeUrl","url":"https://example.com/path"}}],[{"@type":"inlineKeyboardButton","text":"Vote","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeCallback","data":"AQID"}},{"@type":"inlineKeyboardButton","text":"Search here","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeSwitchInline","query":"cats","target_chat":{"@type":"targetChatCurrent"}}}],[{"@type":"inlineKeyboardButton","text":"Mystery","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeQuantum"}}]],"force_reply":false},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Pick one","entities":[]}}}}"#,
        ],
    );
    let message = session
        .histories
        .get(&21)
        .and_then(|h| h.messages.get(&301))
        .expect("message 301 stored");
    let keyboard = message.reply_markup.as_ref().expect("keyboard stored");
    assert_eq!(keyboard.rows.len(), 3);
    match &keyboard.rows[0][0].kind {
        InlineKeyboardButtonType::Url { url } => {
            assert_eq!(url, "https://example.com/path");
            // URL dispatch predicate (same gate the UI uses before OS open).
            assert!(quill::text::openable_http_url(url));
        }
        other => panic!("{other:?}"),
    }
    match &keyboard.rows[1][0].kind {
        InlineKeyboardButtonType::Callback { data } => assert_eq!(data, &[1, 2, 3]),
        other => panic!("{other:?}"),
    }
    match &keyboard.rows[1][1].kind {
        InlineKeyboardButtonType::SwitchInline { query, target } => {
            assert_eq!(query, "cats");
            assert_eq!(*target, InlineKeyboardTargetChat::Current);
        }
        other => panic!("{other:?}"),
    }
    // Unknown button types are kept and render disabled — never a crash.
    assert!(matches!(
        &keyboard.rows[2][0].kind,
        InlineKeyboardButtonType::Unknown { .. }
    ));
}

/// Phase 3.2: a `callbackQueryAnswer` response whose `@extra` matches the
/// in-flight `getCallbackQueryAnswer` request is stored for the UI status
/// line; an answer without a matching request is ignored. TDLib error 502
/// (bot missed the query timeout) surfaces as an honest fallback note.
#[test]
fn replay_callback_query_answer_surfaced() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let extra = session.request(
        RequestPurpose::GetCallbackQueryAnswer,
        Some(quill::ids::ChatId(21)),
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"callbackQueryAnswer","@extra":"{}","text":"Voted!","show_alert":false,"url":""}}"#,
            extra.0
        )],
    );
    let answer = session.last_callback_answer.take().expect("answer stored");
    assert_eq!(answer.text, "Voted!");
    assert!(!answer.show_alert);
    assert!(answer.url.is_empty());
    // No in-flight request: a stray answer is ignored.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"callbackQueryAnswer","@extra":"999","text":"stray","show_alert":false,"url":""}"#,
        ],
    );
    assert!(session.last_callback_answer.is_none());
    // Error on the in-flight request → fallback note, no TDLib text echoed.
    let extra = session.request(
        RequestPurpose::GetCallbackQueryAnswer,
        Some(quill::ids::ChatId(21)),
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"error","@extra":"{}","code":502,"message":"BOT_QUERY_TIMEOUT"}}"#,
            extra.0
        )],
    );
    let answer = session.last_callback_answer.take().expect("fallback note");
    assert_eq!(answer.text, "bot did not answer");
    assert!(answer.url.is_empty());
}

/// Phase 3.2: a bot message carrying `replyMarkupInlineKeyboard` lands on
/// `HistoryMessage.reply_markup` with rows, styles, and button types intact
/// (callback, URL, switchInline, disabled-type buttons all parsed).
#[test]
fn replay_inline_keyboard_mixed_lands_on_history() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            BOT_USER_JSON,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
            r#"{"@type":"updateChatPosition","chat_id":21,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"40","is_pinned":false}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Visit site","icon_custom_emoji_id":0,"style":{"@type":"buttonStylePrimary"},"type":{"@type":"inlineKeyboardButtonTypeUrl","url":"https://example.com"}}],[{"@type":"inlineKeyboardButton","text":"Vote","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleSuccess"},"type":{"@type":"inlineKeyboardButtonTypeCallback","data":"AQID"}},{"@type":"inlineKeyboardButton","text":"Search here","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeSwitchInline","query":"cats","target_chat":{"@type":"targetChatCurrent"}}},{"@type":"inlineKeyboardButton","text":"Buy now","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDanger"},"type":{"@type":"inlineKeyboardButtonTypeBuy"}}]],"force_reply":false},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Pick one","entities":[]}}}}"#,
        ],
    );
    use quill::telegram::envelope::{
        InlineKeyboardButtonStyle, InlineKeyboardButtonType, InlineKeyboardTargetChat,
    };
    let keyboard = session.histories.get(&21).unwrap().messages[&301]
        .reply_markup
        .clone()
        .expect("inline keyboard on history message");
    assert!(!keyboard.force_reply);
    assert_eq!(keyboard.rows.len(), 2);
    assert_eq!(keyboard.rows[0].len(), 1);
    assert_eq!(keyboard.rows[1].len(), 3);
    let visit = &keyboard.rows[0][0];
    assert_eq!(visit.text, "Visit site");
    assert_eq!(visit.style, InlineKeyboardButtonStyle::Primary);
    assert_eq!(
        visit.kind,
        InlineKeyboardButtonType::Url {
            url: "https://example.com".to_string()
        }
    );
    let vote = &keyboard.rows[1][0];
    assert_eq!(vote.style, InlineKeyboardButtonStyle::Success);
    assert_eq!(
        vote.kind,
        InlineKeyboardButtonType::Callback {
            data: vec![1, 2, 3]
        }
    );
    let search = &keyboard.rows[1][1];
    assert_eq!(search.style, InlineKeyboardButtonStyle::Default);
    assert_eq!(
        search.kind,
        InlineKeyboardButtonType::SwitchInline {
            query: "cats".to_string(),
            target: InlineKeyboardTargetChat::Current,
        }
    );
    let buy = &keyboard.rows[1][2];
    assert_eq!(buy.style, InlineKeyboardButtonStyle::Danger);
    assert_eq!(buy.kind, InlineKeyboardButtonType::Buy);
}

/// Phase 3.2: a hostile keyboard (unknown button `@type`, unknown style,
/// missing `type`, non-array rows) never breaks the parse — malformed rows
/// are skipped and malformed buttons become disabled `Unknown` placeholders.
#[test]
fn replay_inline_keyboard_hostile_yields_disabled_placeholders() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            BOT_USER_JSON,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":302,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Mystery","style":{"@type":"buttonStyleFuture"},"type":{"@type":"inlineKeyboardButtonTypeQuantum"}}],[{"@type":"inlineKeyboardButton","text":"No type here"}],"not an array",null],"force_reply":true},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"x","entities":[]}}}}"#,
        ],
    );
    use quill::telegram::envelope::{InlineKeyboardButtonStyle, InlineKeyboardButtonType};
    let keyboard = session.histories.get(&21).unwrap().messages[&302]
        .reply_markup
        .clone()
        .expect("keyboard parsed despite hostile rows");
    assert!(keyboard.force_reply);
    // The two non-array rows are skipped, so only two rows survive.
    assert_eq!(keyboard.rows.len(), 2);
    let mystery = &keyboard.rows[0][0];
    // Unknown style falls back to Default; unknown type → Unknown placeholder.
    assert_eq!(mystery.style, InlineKeyboardButtonStyle::Default);
    assert_eq!(
        mystery.kind,
        InlineKeyboardButtonType::Unknown {
            type_name: "inlineKeyboardButtonTypeQuantum".to_string()
        }
    );
    // Missing `type` → Unknown placeholder (renders disabled).
    assert!(matches!(
        keyboard.rows[1][0].kind,
        InlineKeyboardButtonType::Unknown { .. }
    ));
}

/// Phase 3.2: non-inline markups (`replyMarkupShowKeyboard`) are ignored —
/// `reply_markup` stays `None`, same as an absent field.
#[test]
fn replay_non_inline_markup_ignored() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            BOT_USER_JSON,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":303,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupShowKeyboard","rows":[],"is_persistent":false,"resize_keyboard":false,"one_time":false,"is_personal":false,"force_reply":false,"input_field_placeholder":""},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"x","entities":[]}}}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":304,"chat_id":21,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"y","entities":[]}}}}"#,
        ],
    );
    let history = session.histories.get(&21).unwrap();
    assert!(history.messages[&303].reply_markup.is_none());
    assert!(history.messages[&304].reply_markup.is_none());
}

/// Phase 3.2: `updateMessageEdited` (schema 1.8.67 line 10431) replaces the
/// message's inline keyboard — a new keyboard lands, and a null/absent
/// `reply_markup` removes it.
#[test]
fn replay_update_message_edited_replaces_keyboard() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            BOT_USER_JSON,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Old","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeCallback","data":""}}]],"force_reply":false},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"old","entities":[]}}}}"#,
        ],
    );
    assert!(
        session.histories.get(&21).unwrap().messages[&301]
            .reply_markup
            .is_some()
    );
    // The bot edits the message with a new keyboard: it replaces the old one.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateMessageEdited","chat_id":21,"message_id":301,"edit_date":1700000001,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"New","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDanger"},"type":{"@type":"inlineKeyboardButtonTypeCallback","data":"AA=="}}]],"force_reply":false}}"#,
        ],
    );
    use quill::telegram::envelope::InlineKeyboardButtonStyle;
    let keyboard = session.histories.get(&21).unwrap().messages[&301]
        .reply_markup
        .clone()
        .expect("edited keyboard present");
    assert_eq!(keyboard.rows.len(), 1);
    assert_eq!(keyboard.rows[0].len(), 1);
    assert_eq!(keyboard.rows[0][0].text, "New");
    assert_eq!(keyboard.rows[0][0].style, InlineKeyboardButtonStyle::Danger);
    // The bot edits the message with no reply_markup: the keyboard is gone.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateMessageEdited","chat_id":21,"message_id":301,"edit_date":1700000002,"reply_markup":null}"#,
        ],
    );
    assert!(
        session.histories.get(&21).unwrap().messages[&301]
            .reply_markup
            .is_none()
    );
}

#[test]
fn replay_text_entities_mixed_nested_unknown_and_malformed() {
    use quill::text::{TextEntityKind, styled_runs, utf8_to_utf16_offset};
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);

    let text = "Bold italic bolditalic underline strike secret code\nfn f() {}";
    let span = |needle: &str| -> (i32, i32) {
        let start = text.find(needle).unwrap();
        let s = utf8_to_utf16_offset(text, start).unwrap();
        let e = utf8_to_utf16_offset(text, start + needle.len()).unwrap();
        (s, e - s)
    };
    let ent = |needle: &str, type_json: &str| -> String {
        let (offset, length) = span(needle);
        format!(
            r#"{{"@type":"textEntity","offset":{offset},"length":{length},"type":{type_json}}}"#
        )
    };
    let entities = [
        ent("Bold", r#"{"@type":"textEntityTypeBold"}"#),
        ent("italic", r#"{"@type":"textEntityTypeItalic"}"#),
        ent("bolditalic", r#"{"@type":"textEntityTypeBold"}"#),
        ent("bolditalic", r#"{"@type":"textEntityTypeItalic"}"#),
        ent("underline", r#"{"@type":"textEntityTypeUnderline"}"#),
        ent("strike", r#"{"@type":"textEntityTypeStrikethrough"}"#),
        ent("secret", r#"{"@type":"textEntityTypeSpoiler"}"#),
        ent("code", r#"{"@type":"textEntityTypeCode"}"#),
        ent(
            "fn f() {}",
            r#"{"@type":"textEntityTypePreCode","language":"rust"}"#,
        ),
        // Unknown types are ignored, never crash the parse.
        ent("Bold", r#"{"@type":"textEntityTypeMention"}"#),
        ent("Bold", r#"{"@type":"textEntityTypeBlockQuote"}"#),
        ent(
            "Bold",
            r#"{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":"123"}"#,
        ),
        // Malformed offsets are dropped (zero length, past the end).
        r#"{"@type":"textEntity","offset":5,"length":0,"type":{"@type":"textEntityTypeBold"}}"#
            .to_string(),
        r#"{"@type":"textEntity","offset":500,"length":10,"type":{"@type":"textEntityTypeBold"}}"#
            .to_string(),
    ]
    .join(",");
    let text_json = serde_json::to_string(text).unwrap();
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":30,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text_json},"entities":[{entities}]}}}}}}}}"#
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            &json,
        ],
    );
    let message = session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&30)
        .unwrap();
    let quill::telegram::envelope::MessageContent::Text(content) = &message.content else {
        panic!("expected text");
    };
    assert_eq!(content.text, text);
    let kinds: Vec<&TextEntityKind> = content.entities.iter().map(|e| &e.kind).collect();
    assert_eq!(kinds.len(), 9, "unknown + malformed entities are dropped");
    assert!(matches!(kinds[0], TextEntityKind::Bold));
    assert!(matches!(kinds[1], TextEntityKind::Italic));
    assert!(matches!(kinds[2], TextEntityKind::Bold));
    assert!(matches!(kinds[3], TextEntityKind::Italic));
    assert!(matches!(kinds[4], TextEntityKind::Underline));
    assert!(matches!(kinds[5], TextEntityKind::Strikethrough));
    assert!(matches!(kinds[6], TextEntityKind::Spoiler));
    assert!(matches!(kinds[7], TextEntityKind::Code));
    assert!(matches!(
        kinds[8],
        TextEntityKind::PreCode { language } if language == "rust"
    ));

    // Nesting combines: the "bolditalic" span renders bold italic.
    let runs = styled_runs(&content.text, &content.entities);
    let nested = runs
        .iter()
        .find(|run| run.text == "bolditalic")
        .expect("nested run");
    assert!(nested.style.bold && nested.style.italic);
    // Pre keeps its language; spoiler and code spans split correctly.
    let pre = runs
        .iter()
        .find(|run| run.text == "fn f() {}")
        .expect("pre run");
    assert!(pre.style.pre);
    assert_eq!(pre.style.language.as_deref(), Some("rust"));
    let spoiler = runs
        .iter()
        .find(|run| run.text == "secret")
        .expect("spoiler run");
    assert!(spoiler.style.spoiler);
    let code = runs
        .iter()
        .find(|run| run.text == "code")
        .expect("code run");
    assert!(code.style.code);
}

#[test]
fn replay_photo_caption_carries_entities() {
    use quill::text::{TextEntityKind, styled_runs};
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let caption = "cap bold plus link https://example.com/x";
    let link_at = caption.find("https").unwrap();
    let file = r#"{"@type":"file","id":11,"size":10,"expected_size":10,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}}"#;
    let caption_json = serde_json::to_string(caption).unwrap();
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":31,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":320,"height":240,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":{caption_json},"entities":[{{"@type":"textEntity","offset":4,"length":4,"type":{{"@type":"textEntityTypeBold"}}}},{{"@type":"textEntity","offset":{link_at},"length":21,"type":{{"@type":"textEntityTypeUrl"}}}}]}},"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            &json,
        ],
    );
    let message = session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&31)
        .unwrap();
    let quill::telegram::envelope::MessageContent::Photo(photo) = &message.content else {
        panic!("expected photo");
    };
    assert_eq!(photo.caption, caption);
    assert_eq!(photo.caption_entities.len(), 2);
    assert!(matches!(
        photo.caption_entities[0].kind,
        TextEntityKind::Bold
    ));
    assert!(matches!(
        photo.caption_entities[1].kind,
        TextEntityKind::Url
    ));
    let runs = styled_runs(&photo.caption, &photo.caption_entities);
    let bold = runs
        .iter()
        .find(|run| run.text == "bold")
        .expect("bold run");
    assert!(bold.style.bold);
    let link = runs
        .iter()
        .find(|run| run.text == "https://example.com/x")
        .expect("link run");
    assert_eq!(link.href.as_deref(), Some("https://example.com/x"));
    assert!(!sink.rendered().contains("CANARY_REMOTE"));
}

#[test]
fn replay_click_photo_opens_viewer_with_largest_file_id() {
    // Phase 4.5: clicking a photo message opens the viewer on the clicked
    // message's item, with the largest size's file id as the
    // download/display target.
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let file = |id: i32| {
        format!(
            r#"{{"@type":"file","id":{id},"size":10,"expected_size":10,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}}}}"#
        )
    };
    let sizes = format!(
        "{{\"@type\":\"photoSize\",\"type\":\"m\",\"photo\":{thumb},\"width\":320,\"height\":240,\"progressive_sizes\":[]}},{{\"@type\":\"photoSize\",\"type\":\"x\",\"photo\":{full},\"width\":1280,\"height\":960,\"progressive_sizes\":[]}}",
        thumb = file(1),
        full = file(2),
    );
    let photo_message = format!(
        "{{\"@type\":\"updateNewMessage\",\"message\":{{\"id\":20,\"chat_id\":7,\"is_outgoing\":false,\"content\":{{\"@type\":\"messagePhoto\",\"photo\":{{\"@type\":\"photo\",\"has_stickers\":false,\"sizes\":[{sizes}]}},\"caption\":{{\"@type\":\"formattedText\",\"text\":\"CANARY_REPLAY_viewer\",\"entities\":[]}},\"has_spoiler\":false,\"is_secret\":false}}}}}}",
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            &photo_message,
        ],
    );
    let history = session.histories.get(&7).expect("chat history");
    let messages: Vec<quill::state::HistoryMessage> =
        history.ordered().into_iter().cloned().collect();
    // The click handler builds the viewer list from the chat's media
    // messages, then opens on the clicked message's index.
    let items = quill::media_viewer::collect_media_items(&messages);
    assert_eq!(items.len(), 1);
    let index = items
        .iter()
        .position(|item| item.message_id == quill::ids::MessageId(20))
        .expect("clicked photo is in the viewer list");
    let viewer = quill::media_viewer::MediaViewer::open(items, index);
    let current = viewer.current().expect("viewer is open");
    assert_eq!(current.download_file_id, quill::ids::FileId(2));
    assert_eq!(current.display_file_ids[0], quill::ids::FileId(2));
    assert!(current.display_file_ids.contains(&quill::ids::FileId(1)));
    assert_eq!(current.caption, "CANARY_REPLAY_viewer");
    assert!(!sink.rendered().contains("CANARY_REPLAY"));
    assert!(!sink.rendered().contains("CANARY_REMOTE"));
}

/// Phase 6: the contacts flow — `updateUser` objects land the user cache,
/// a `getContacts` `users` response lands the id list, `contact_rows`
/// renders name/status rows in server order sorted by name, and a
/// `getUserFullInfo` response (correlated via the pending request's
/// user id) caches the bio plus the preferred `chatPhoto` file. A later
/// `updateUserStatus` refreshes the row's status text.
#[test]
fn replay_contacts_list_and_user_full_info() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","last_name":"Lovelace","phone_number":"+15550101031","status":{"@type":"userStatusOnline","expires":9999999999},"is_contact":true,"type":{"@type":"userTypeRegular"}}}"#,
            r#"{"@type":"updateUser","user":{"id":32,"first_name":"Zed","last_name":"Hopper","phone_number":"+15550101032","status":{"@type":"userStatusLastWeek","by_my_privacy_settings":false},"is_contact":false,"type":{"@type":"userTypeRegular"}}}"#,
        ],
    );
    assert!(!session.contacts_settled());
    let extra = session.request(RequestPurpose::GetContacts, None);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"users","@extra":"{}","total_count":2,"user_ids":[31,32]}}"#,
            extra.0
        )],
    );
    assert!(session.contacts_settled());
    let rows = session.contact_rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].name, "Ada Lovelace");
    assert!(rows[0].is_online);
    assert!(rows[0].is_contact);
    assert_eq!(rows[1].name, "Zed Hopper");
    assert!(!rows[1].is_online);
    assert!(!rows[1].is_contact);
    assert_eq!(rows[1].status_text, "last seen within a week");

    // Full info for Zed: bio + chatPhoto (preferred "m" size wins).
    let extra = session.request_for_user(RequestPurpose::GetUserFullInfo, 32);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bio":{{"@type":"formattedText","text":"CANARY_REPLAY_bio","entities":[]}},"bot_info":null,"photo":{{"@type":"chatPhoto","id":1,"added_date":1,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"s","photo":{{"@type":"file","id":901}},"width":90,"height":90,"progressive_sizes":[]}},{{"@type":"photoSize","type":"m","photo":{{"@type":"file","id":902}},"width":320,"height":320,"progressive_sizes":[]}}],"animation":null,"small_animation":null,"sticker":null}}}}"#,
            extra.0
        )],
    );
    let info = session.user_full_info(32).expect("full info cached");
    assert_eq!(info.bio, "CANARY_REPLAY_bio");
    assert_eq!(info.photo_file_id, Some(902));
    assert!(session.files.contains_key(&902));

    // A later status update refreshes the contact row.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateUserStatus","user_id":32,"status":{"@type":"userStatusOnline","expires":9999999999}}"#,
        ],
    );
    let rows = session.contact_rows();
    assert!(rows[1].is_online);
    assert_eq!(rows[1].status_text, "online");
    assert!(!sink.rendered().contains("CANARY_REPLAY_bio"));
}

/// Phase C1: the full call-signaling lifecycle through the reducer —
/// incoming pending → exchanging keys → ready (with honestly queued
/// signaling data) → discarded (summary + rating flag); a second
/// incoming call while one is active is queued for busy-decline; a
/// failed `createCall` surfaces `call_error`.
#[test]
fn replay_call_signaling_lifecycle() {
    use quill::telegram::envelope::CallState;

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);

    // Incoming pending call from Zed (user 41).
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
        ],
    );
    let call = session.active_call.as_ref().expect("incoming call tracked");
    assert_eq!(call.id, 77);
    assert_eq!(call.user_id, 41);
    assert!(!call.is_outgoing);
    assert!(matches!(
        call.state,
        CallState::Pending {
            is_created: true,
            is_received: false
        }
    ));
    assert!(session.call_summary.is_none());

    // A second incoming call while one is active → busy-decline queue.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateCall","call":{"@type":"call","id":78,"unique_id":"100","user_id":42,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
        ],
    );
    assert_eq!(session.active_call.as_ref().expect("still call 77").id, 77);
    assert_eq!(session.call_busy_decline_queue, vec![78]);

    // Keys exchange, then Ready; signaling data is queued honestly.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStateExchangingKeys"}}}"#,
            r#"{"@type":"updateNewCallSignalingData","call_id":77,"data":"AAEC"}"#,
            r#"{"@type":"updateNewCallSignalingData","call_id":78,"data":"AAEC"}"#,
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStateReady","protocol":{"@type":"callProtocol","udp_p2p":true,"udp_reflector":true,"min_layer":65,"max_layer":92,"library_versions":[]},"servers":[],"config":"{}","encryption_key":"","emojis":[],"allow_p2p":false,"is_group_call_supported":false,"custom_parameters":"{}"}}}"#,
        ],
    );
    let call = session.active_call.as_ref().expect("call 77 ready");
    assert_eq!(call.state, CallState::Ready);
    assert!(call.ready_at.is_some());
    // Only the tracked call's signaling data is kept (call 78's is
    // dropped — no tracked call with that id).
    assert_eq!(call.signaling_queue.len(), 1);
    assert_eq!(call.signaling_queue[0], vec![0x00, 0x01, 0x02]);

    // Remote hangup with need_rating → summary drives the end screen.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":true,"need_debug_information":false,"need_log":false}}}"#,
        ],
    );
    assert!(session.active_call.is_none());
    let summary = session.call_summary.as_ref().expect("end summary");
    assert_eq!(summary.call_id, 77);
    assert_eq!(summary.end_line, "Call ended");
    assert!(summary.need_rating);
    assert!(!summary.rating_sent);
    assert!(!summary.need_debug_information);
    assert!(!summary.need_log);

    // A rejected `createCall` surfaces the error (TDLib's message text
    // is never stored — it can contain secrets).
    let extra = session.request_for_user(RequestPurpose::CreateCall, 41);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"PHONE_CALL_PROTOCOL_ERROR"}}"#,
            extra.0
        )],
    );
    assert!(session.active_call.is_none());
    let error = session.call_error.as_ref().expect("call error shown");
    assert!(error.contains("Could not start the call"));
    assert!(error.contains("400"));
    assert!(!error.contains("PHONE_CALL_PROTOCOL_ERROR"));

    // The `callId` answer starts tracking the outgoing call.
    let extra = session.request_for_user(RequestPurpose::CreateCall, 41);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"callId","@extra":"{}","id":79}}"#,
            extra.0
        )],
    );
    let call = session.active_call.as_ref().expect("outgoing tracked");
    assert_eq!(call.id, 79);
    assert!(call.is_outgoing);
    // The failed request did not leave a tracked call behind earlier,
    // and the error is cleared when a new call is tracked.
    assert!(session.call_error.is_none());

    // Hang up the outgoing call.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateCall","call":{"@type":"call","id":79,"unique_id":"103","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":false,"need_debug_information":false,"need_log":false}}}"#,
        ],
    );
    assert!(session.active_call.is_none());

    // A missed incoming call we never tracked still records a summary.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateCall","call":{"@type":"call","id":80,"unique_id":"101","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonMissed"},"need_rating":false,"need_debug_information":false,"need_log":false}}}"#,
        ],
    );
    let summary = session.call_summary.as_ref().expect("missed summary");
    assert_eq!(summary.end_line, "Missed call");

    // A call error with the documented 4005000 timeout code.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateCall","call":{"@type":"call","id":81,"unique_id":"102","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStateError","error":{"@type":"error","code":4005000,"message":"CALL_TIMEOUT"}}}}"#,
        ],
    );
    let summary = session.call_summary.as_ref().expect("error summary");
    assert!(summary.end_line.contains("timed out"));
}
