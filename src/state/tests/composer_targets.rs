//! The reply and edit bars read their message from the session.
use super::common::*;
use super::*;
use crate::composer::{ComposerEdit, ForwardDraft};

const NEW: &str = r#"{"@type":"updateNewMessage","message":{"id":50,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"first words","entities":[]}}}}"#;
const EDITED: &str = r#"{"@type":"updateMessageContent","chat_id":7,"message_id":50,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"second words","entities":[]}}}"#;
const DELETED: &str = r#"{"@type":"updateDeleteMessages","chat_id":7,"message_ids":[50],"is_permanent":true,"from_cache":false}"#;
const GONE_FROM_CACHE: &str = r#"{"@type":"updateDeleteMessages","chat_id":7,"message_ids":[50],"is_permanent":false,"from_cache":true}"#;

fn seeded() -> (Session, Arc<MemorySink>, AtomicU64) {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(7));
    apply_json(&mut session, &seq, &sink, NEW);
    (session, sink, seq)
}

fn own_edit() -> ComposerEdit {
    ComposerEdit::from_own_content(
        ChatId(7),
        MessageId(50),
        true,
        false,
        &MessageContent::Text("anything".into()),
    )
    .expect("own text is editable")
}

#[test]
fn a_reply_reads_the_edited_message_not_a_copy() {
    let (mut session, sink, seq) = seeded();
    let preview = |session: &Session| {
        session
            .reply_target(ChatId(7), MessageId(50))
            .map(effective_preview)
    };
    assert_eq!(preview(&session).as_deref(), Some("first words"));
    apply_json(&mut session, &seq, &sink, EDITED);
    assert_eq!(preview(&session).as_deref(), Some("second words"));
}

#[test]
fn an_edit_bar_reads_the_edited_text() {
    let (mut session, sink, seq) = seeded();
    let edit = own_edit();
    assert_eq!(
        session.edit_target_text(&edit).as_deref(),
        Some("first words")
    );
    apply_json(&mut session, &seq, &sink, EDITED);
    assert_eq!(
        session.edit_target_text(&edit).as_deref(),
        Some("second words")
    );
}

#[test]
fn a_delete_marks_the_reply_and_edit_targets_gone() {
    let (mut session, sink, seq) = seeded();
    let edit = own_edit();
    assert!(!session.message_deleted(ChatId(7), MessageId(50)));
    assert!(!session.edit_target_gone(&edit));
    apply_json(&mut session, &seq, &sink, DELETED);
    assert!(session.message_deleted(ChatId(7), MessageId(50)));
    assert!(session.edit_target_gone(&edit));
    assert!(session.reply_target(ChatId(7), MessageId(50)).is_none());
}

#[test]
fn a_message_that_is_only_not_loaded_is_not_gone() {
    let (session, _sink, _seq) = seeded();
    assert!(!session.message_deleted(ChatId(7), MessageId(999)));
    assert!(session.reply_target(ChatId(7), MessageId(999)).is_none());
    // A cache drop is not a delete either.
    let (mut session, sink, seq) = seeded();
    apply_json(&mut session, &seq, &sink, GONE_FROM_CACHE);
    assert!(!session.message_deleted(ChatId(7), MessageId(50)));
}

#[test]
fn a_selected_message_stops_counting_when_it_is_deleted() {
    let (mut session, sink, seq) = seeded();
    let mut draft = ForwardDraft::from_message(ChatId(7), MessageId(50), false).expect("sent id");
    draft.toggle(ChatId(7), MessageId(51), false);
    let alive = |session: &Session, draft: &ForwardDraft| {
        draft
            .clone()
            .retaining(|id| session.message_still_there(ChatId(7), id))
            .message_ids
    };
    // 51 was never loaded: with the chat's history loaded it is not there.
    assert_eq!(alive(&session, &draft), vec![MessageId(50)]);
    apply_json(&mut session, &seq, &sink, DELETED);
    assert!(alive(&session, &draft).is_empty());
    // A chat with no loaded history cannot say: its ids stay.
    assert_eq!(
        draft
            .clone()
            .retaining(|id| session.message_still_there(ChatId(99), id))
            .count(),
        2
    );
}
