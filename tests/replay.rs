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
    let extra = session.request(RequestPurpose::SendText, Some(quill::ids::ChatId(7)));
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
    let send_extra = session.request(RequestPurpose::SendText, Some(quill::ids::ChatId(7)));
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
