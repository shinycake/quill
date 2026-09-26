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
        ],
    );
    assert_eq!(session.search.status, quill::state::SearchStatus::Ready);
    assert_eq!(session.search.chat_ids[0].0, 7);
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
        ],
    );
    assert_eq!(session.search.status, quill::state::SearchStatus::Empty);

    let search_gen = session.search.begin_query("nope");
    let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
    let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
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

/// Phase 2.2: the composer stays hidden in channels; own membership flows
/// through `getMe` / `getChatMember` / `joinChat` / `updateChatMember` /
/// `leaveChat`.
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

    // updateChatMember confirms the admin promotion (composer still hidden in 2.2).
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
    assert!(!chat.can_post());

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
    chat.set_member_status(ChannelMemberStatus::Left);
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
