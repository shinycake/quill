//! Chat TTL / auto-delete timers replay tests.
//! Split from `tests/replay.rs` — pure code motion.
mod replay_common;
use replay_common::*;

/// Phase B4: chat-level auto-delete / self-destruct timer
/// (`chat.message_auto_delete_time`, schema 1.8.67 lines 3616 / 3627)
/// arrives with `updateNewChat` and refreshes via
/// `updateChatMessageAutoDeleteTime` (line 10549).
#[test]
fn replay_chat_ttl_initial_and_live_update() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":31,"user_id":7,"state":{"@type":"secretChatStateReady"},"is_outbound":true,"key_hash":"","layer":144}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Secret","type":{"@type":"chatTypeSecret","secret_chat_id":31,"user_id":7},"unread_count":0,"message_auto_delete_time":3600}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":32,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
        ],
    );
    let secret = session.chats.get(&31).unwrap();
    assert_eq!(secret.message_auto_delete_time, 3600);
    assert_eq!(
        secret.ttl_status_line().as_deref(),
        Some("Self-destruct: 1h")
    );
    // No timer on the regular chat: status line hidden.
    let peer = session.chats.get(&32).unwrap();
    assert_eq!(peer.message_auto_delete_time, 0);
    assert_eq!(peer.ttl_status_line(), None);

    // Live update turns the timer off.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateChatMessageAutoDeleteTime","chat_id":31,"message_auto_delete_time":0}"#,
        ],
    );
    let secret = session.chats.get(&31).unwrap();
    assert_eq!(secret.message_auto_delete_time, 0);
    assert_eq!(secret.ttl_status_line(), None);

    // ...and back on with a day-multiple (regular-chat shape).
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateChatMessageAutoDeleteTime","chat_id":32,"message_auto_delete_time":86400}"#,
        ],
    );
    let peer = session.chats.get(&32).unwrap();
    assert_eq!(peer.ttl_status_line().as_deref(), Some("Auto-delete: 1d"));
}

/// Phase B4: `messageChatSetMessageAutoDeleteTime` (schema 1.8.67,
/// line 5387) lands in history as the typed service-row content.
#[test]
fn replay_chat_ttl_service_message_row() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":31,"user_id":7,"state":{"@type":"secretChatStateReady"},"is_outbound":true,"key_hash":"","layer":144}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Secret","type":{"@type":"chatTypeSecret","secret_chat_id":31,"user_id":7},"unread_count":0,"message_auto_delete_time":3600}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":701,"chat_id":31,"is_outgoing":false,"content":{"@type":"messageChatSetMessageAutoDeleteTime","message_auto_delete_time":3600,"from_user_id":7}}}"#,
        ],
    );
    let history = session.histories.get(&31).unwrap();
    let row = history.messages.get(&701).expect("service row in history");
    assert!(matches!(
        row.content,
        quill::telegram::envelope::MessageContent::ChatTtlChanged { secs: 3600 }
    ));
}

/// Phase B4: `message.auto_delete_in` (schema 1.8.67, line 3148)
/// reaches the history row as a live countdown; the row itself leaves
/// through the normal `updateDeleteMessages` path.
#[test]
fn replay_chat_ttl_auto_delete_in_and_delete() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":31,"user_id":7,"state":{"@type":"secretChatStateReady"},"is_outbound":true,"key_hash":"","layer":144}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Secret","type":{"@type":"chatTypeSecret","secret_chat_id":31,"user_id":7},"unread_count":0,"message_auto_delete_time":3600}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":702,"chat_id":31,"is_outgoing":false,"auto_delete_in":3595.5,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"ephemeral","entities":[]}}}}"#,
        ],
    );
    let history = session.histories.get(&31).unwrap();
    let row = history.messages.get(&702).expect("message in history");
    let auto_delete = row.auto_delete.expect("auto_delete_in parsed");
    assert_eq!(auto_delete.expires_in_ms, 3_595_500);
    assert!(row.has_live_auto_delete(quill::state::unix_ms_now()));

    // TDLib deletes the row when the timer fires — normal path.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateDeleteMessages","chat_id":31,"message_ids":[702],"is_permanent":true,"from_cache":false}"#,
        ],
    );
    let history = session.histories.get(&31).unwrap();
    assert!(!history.contains(quill::ids::MessageId(702)));
}
