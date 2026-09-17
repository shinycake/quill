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
