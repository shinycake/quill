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
            .retaining(|id| !session.message_deleted(ChatId(7), id))
            .message_ids
    };
    // 51 is not loaded but was not deleted either: it stays picked.
    assert_eq!(alive(&session, &draft), vec![MessageId(50), MessageId(51)]);
    apply_json(&mut session, &seq, &sink, DELETED);
    assert_eq!(alive(&session, &draft), vec![MessageId(51)]);
    // A chat with no loaded history cannot say: its ids stay.
    assert_eq!(
        draft
            .clone()
            .retaining(|id| !session.message_deleted(ChatId(99), id))
            .count(),
        2
    );
}

const NEWER: &str = r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"later","entities":[]}}}}"#;

#[test]
fn a_selection_survives_the_window_trim_and_a_window_reset() {
    let (mut session, sink, seq) = seeded();
    apply_json(&mut session, &seq, &sink, NEWER);
    let mut draft = ForwardDraft::from_message(ChatId(7), MessageId(50), false).expect("sent id");
    draft.toggle(ChatId(7), MessageId(60), false);
    let alive = |session: &Session, draft: &ForwardDraft| {
        draft
            .clone()
            .retaining(|id| !session.message_deleted(ChatId(7), id))
            .count()
    };
    // The oldest row falls out of the window: unloaded, not deleted.
    let trimmed = session
        .histories
        .get_mut(&7)
        .map(|h| h.trim_window_end(WindowEnd::Oldest, 1, 1));
    assert_eq!(trimmed, Some(1));
    assert!(session.reply_target(ChatId(7), MessageId(50)).is_none());
    assert_eq!(alive(&session, &draft), 2, "a trim does not unselect");
    // A jump replaces the whole window.
    if let Some(history) = session.histories.get_mut(&7) {
        history.reset_window();
    }
    assert_eq!(alive(&session, &draft), 2, "nor does a window reset");
    // A real delete does.
    apply_json(&mut session, &seq, &sink, DELETED);
    assert_eq!(alive(&session, &draft), 1);
}

#[test]
fn a_delete_is_a_tombstone_in_the_open_thread_and_every_topic_of_the_chat() {
    let (mut session, sink, seq) = seeded();
    let mut thread = ThreadView::resolving(ChatId(7), MessageId(1));
    thread.status = ThreadStatus::Ready;
    session.threads.thread = Some(thread);
    session
        .threads
        .topic_histories
        .insert((7, 2), Default::default());
    apply_json(&mut session, &seq, &sink, DELETED);
    let in_thread = session
        .threads
        .thread
        .as_ref()
        .is_some_and(|thread| thread.history.is_tombstone(MessageId(50)));
    assert!(in_thread, "the thread remembers the delete");
    assert!(session.threads.topic_histories[&(7, 2)].is_tombstone(MessageId(50)));
}

#[test]
fn a_message_deleted_only_in_the_thread_or_topic_it_was_found_in_is_gone() {
    // `reply_target` looks in the chat, the open thread and the open topic:
    // "deleted" answers for the same places.
    let (mut session, _sink, _seq) = seeded();
    assert!(!session.message_deleted(ChatId(7), MessageId(60)));
    let mut thread = ThreadView::resolving(ChatId(7), MessageId(1));
    thread.status = ThreadStatus::Ready;
    thread.history.remove(MessageId(60), true);
    session.threads.thread = Some(thread);
    assert!(session.message_deleted(ChatId(7), MessageId(60)));
    // Another chat's thread says nothing about this chat.
    assert!(!session.message_deleted(ChatId(8), MessageId(60)));

    session.threads.thread = None;
    assert!(!session.message_deleted(ChatId(7), MessageId(60)));
    let mut topic = TopicHistory::default();
    topic.remove(MessageId(60), true);
    session.threads.topic_histories.insert((7, 3), topic);
    assert!(session.message_deleted(ChatId(7), MessageId(60)));
    assert!(!session.message_deleted(ChatId(9), MessageId(60)));
}

fn reject(session: &mut Session, code: i32) {
    session.begin_composer_reply_fetch(ChatId(7), MessageId(999));
    session.reject_composer_reply_target(ChatId(7), MessageId(999), &TdError::from_code(code));
}

#[test]
fn only_a_definite_answer_marks_the_reply_target_gone() {
    for code in [400, 403, 404] {
        let (mut session, _sink, _seq) = seeded();
        reject(&mut session, code);
        assert!(session.message_deleted(ChatId(7), MessageId(999)), "{code}");
    }
    // A lost request (408), a flood wait and a server fault do not cancel
    // the user's reply.
    for code in [408, 429, 500] {
        let (mut session, _sink, _seq) = seeded();
        reject(&mut session, code);
        assert!(
            !session.message_deleted(ChatId(7), MessageId(999)),
            "{code}"
        );
    }
}

#[test]
fn a_failed_reply_fetch_is_retried_when_online_with_a_wait_and_a_limit() {
    use std::time::{Duration, Instant};
    let (mut session, _sink, _seq) = seeded();
    session.connection = ConnectionState::Ready;
    let wanted = |session: &Session| session.composer_reply_fetch_wanted(ChatId(7), MessageId(999));
    assert!(wanted(&session));
    reject(&mut session, 408);
    assert!(!wanted(&session), "no retry on the next frame");

    let age = |session: &mut Session, secs: u64| {
        if let Some(ComposerReplyFetch::Failed { at, .. }) =
            session.messages.composer_reply_fetch.get_mut(&(7, 999))
        {
            *at = Instant::now()
                .checked_sub(Duration::from_secs(secs))
                .unwrap_or_else(Instant::now);
        }
    };
    age(&mut session, 3600);
    session.connection = ConnectionState::WaitingForNetwork;
    assert!(!wanted(&session), "not while offline");
    session.connection = ConnectionState::Ready;
    assert!(wanted(&session), "back online after the wait");

    // Three attempts in all, then it stays failed.
    reject(&mut session, 408);
    age(&mut session, 3600);
    assert!(wanted(&session));
    reject(&mut session, 408);
    age(&mut session, 3600);
    assert!(!wanted(&session), "bounded");
}

#[test]
fn the_fetch_cap_keeps_the_in_flight_and_the_new_entry() {
    let (mut session, _sink, _seq) = seeded();
    session.begin_composer_reply_fetch(ChatId(7), MessageId(1));
    for id in 2..=40 {
        session.begin_composer_reply_fetch(ChatId(7), MessageId(id));
        session.reject_composer_reply_target(ChatId(7), MessageId(id), &TdError::from_code(500));
    }
    let fetch = &session.messages.composer_reply_fetch;
    assert!(fetch.len() <= 32);
    assert!(matches!(
        fetch.get(&(7, 1)),
        Some(ComposerReplyFetch::Loading { .. })
    ));
    assert!(fetch.contains_key(&(7, 40)));
}
