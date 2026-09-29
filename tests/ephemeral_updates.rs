//! Slice `parity:msg-ephemeral-updates` (review fix-up): the slice tests
//! live here instead of the waived files (`src/state.rs` and
//! `src/telegram/envelope.rs` must not grow with new tests). Proves
//! through the public parser/reducer interfaces:
//! 1. the synthetic `updateMessageEphemeralContent` lands on the typed
//!    payload with chat/message ids and the new `ephemeralMessageContent`
//!    (schema 1.8.67, `td_api.tl:10424`).
//! 2. applying it replaces the stored ephemeral content in place and the
//!    chat-list preview shows the refreshed ephemeral text (the row
//!    re-renders via `effective_content`, ephemeral wins).

use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::AccountKey;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{EnvelopePayload, MessageContent, parse_envelope};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn session() -> (Session, Arc<MemorySink>) {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    (Session::new(AccountKey::primary(), dyn_sink), sink)
}

fn apply(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, json: &str) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let owned = copy_and_parse(json, seq, &dyn_sink).unwrap();
    session.apply(owned);
}

#[test]
fn update_message_ephemeral_content_is_typed() {
    let env = parse_envelope(
        r#"{"@type":"updateMessageEphemeralContent","chat_id":11,"message_id":102,"ephemeral_content":{"@type":"ephemeralMessageContent","content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_EPHEMERAL","entities":[]}},"reply_markup":null}}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::UpdateMessageEphemeralContent {
            chat_id,
            message_id,
            ephemeral,
        } => {
            assert_eq!(chat_id.0, 11);
            assert_eq!(message_id.0, 102);
            assert_eq!(
                ephemeral.content.as_ref(),
                &MessageContent::Text("CANARY_EPHEMERAL".into())
            );
            assert!(ephemeral.reply_markup.is_none());
        }
        other => panic!("{other:?}"),
    }
    let schema = include_str!("../schema/td_api.tl");
    assert!(
        schema
            .lines()
            .any(|l| l.starts_with("updateMessageEphemeralContent "))
    );
}

#[test]
fn update_message_ephemeral_content_refreshes_stored_content_and_preview() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"c","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
    );
    apply(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":44,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"public","entities":[]}}}}"#,
    );
    apply(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatLastMessage","chat_id":7,"last_message":{"id":44,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"public","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"8","is_pinned":false}]}"#,
    );
    assert_eq!(session.chats.get(&7).unwrap().last_preview, "public");
    apply(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageEphemeralContent","chat_id":7,"message_id":44,"ephemeral_content":{"@type":"ephemeralMessageContent","content":{"@type":"messageText","text":{"@type":"formattedText","text":"secret v2","entities":[]}},"reply_markup":null}}"#,
    );
    let message = session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&44)
        .unwrap();
    assert_eq!(
        message
            .ephemeral
            .as_ref()
            .expect("ephemeral set")
            .content
            .as_ref(),
        &MessageContent::Text("secret v2".into())
    );
    assert_eq!(session.chats.get(&7).unwrap().last_preview, "secret v2");
}
