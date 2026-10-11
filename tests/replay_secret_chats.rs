//! Secret chats replay tests.
//! Split from `tests/replay.rs` — pure code motion.
mod replay_common;
use replay_common::*;

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
    assert!(session.users_state.secret_chat_fetch_queue.contains(&31));
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
            .users_state
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
    assert!(session.users_state.secret_chat_fetch_queue.is_empty());
}
