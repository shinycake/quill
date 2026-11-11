use super::apply_demo_json;
use crate::ui::demo::seed_ready_chats_session;
use quill::diagnostics::MemorySink;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn a_demo_envelope_is_never_numbered_below_what_the_session_took() {
    let sink = Arc::new(MemorySink::new());
    let mut session = seed_ready_chats_session(sink.clone());
    assert!(session.last_seq > 0, "the seed applied its fixtures");
    // A counter a demo action starts with, behind the session's.
    let seq = AtomicU64::new(0);
    let before = session
        .chats
        .get(&11)
        .map(|chat| chat.is_marked_as_unread)
        .unwrap_or(true);
    assert!(!before);
    let json = r#"{"@type":"updateChatIsMarkedAsUnread","chat_id":11,"is_marked_as_unread":true}"#;
    apply_demo_json(&mut session, &seq, &sink, json);
    assert!(
        session
            .chats
            .get(&11)
            .is_some_and(|chat| chat.is_marked_as_unread),
        "the envelope was applied, not dropped as out of order"
    );
}
