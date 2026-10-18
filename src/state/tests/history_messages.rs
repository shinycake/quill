//! `HistoryMessages` tells a view which messages changed in place and
//! when the set of messages changed, so one edit costs one row.
use super::{ChatId, HistoryMessages, MessageContent, MessageId, ParsedMessage, history_message};

fn message(id: i64, text: &str) -> super::HistoryMessage {
    history_message(
        ParsedMessage {
            sender: None,
            id: MessageId(id),
            chat_id: ChatId(1),
            date: 0,
            is_outgoing: false,
            is_pinned: false,
            topic_id: None,
            thread_id: None,
            ephemeral: None,
            media_album_id: 0,
            author_signature: None,
            scheduling_state: None,
            can_retry: false,
            send_state: Default::default(),
            content: MessageContent::Text(text.into()),
            files: Vec::new(),
            reply_to: None,
            forward_info: None,
            extras: Default::default(),
            interaction_info: None,
            reply_markup: None,
            self_destruct: None,
            auto_delete: None,
        },
        false,
    )
}

fn three() -> HistoryMessages {
    let mut messages = HistoryMessages::default();
    for id in 1..=3 {
        messages.insert(id, message(id, "m"));
    }
    messages
}

#[test]
fn an_edit_names_only_its_message_and_keeps_the_ids() {
    let mut messages = three();
    let (revision, ids) = (messages.revision(), messages.ids_revision());
    if let Some(edited) = messages.get_mut(&2) {
        edited.content = MessageContent::Text("edited".into());
    }
    assert!(messages.revision() > revision);
    assert_eq!(messages.ids_revision(), ids);
    assert_eq!(messages.changed_since(revision), vec![2]);
    // A new snapshot of a known message is a change in place too.
    let revision = messages.revision();
    messages.insert(3, message(3, "again"));
    assert_eq!(messages.ids_revision(), ids);
    assert_eq!(messages.changed_since(revision), vec![3]);
}

#[test]
fn asking_for_a_missing_message_changes_nothing() {
    let mut messages = three();
    let revision = messages.revision();
    assert!(messages.get_mut(&99).is_none());
    assert_eq!(messages.revision(), revision);
    messages.retain(|_, _| true);
    assert_eq!(messages.revision(), revision);
}

#[test]
fn a_message_coming_or_going_moves_the_ids_revision() {
    let mut messages = three();
    let ids = messages.ids_revision();
    messages.insert(4, message(4, "new"));
    let added = messages.ids_revision();
    assert!(added > ids);
    messages.remove(&1);
    assert!(messages.ids_revision() > added);
    let before = messages.ids_revision();
    messages.retain(|id, _| *id != 2);
    assert!(messages.ids_revision() > before);
    assert_eq!(messages.keys().copied().collect::<Vec<_>>(), vec![3, 4]);
    assert!(
        messages
            .changed_since(0)
            .iter()
            .all(|id| *id == 3 || *id == 4)
    );
}

#[test]
fn changes_name_the_messages_added_and_removed() {
    let mut messages = three();
    let revision = messages.revision();
    messages.insert(4, message(4, "new"));
    messages.remove(&2);
    assert_eq!(
        messages.changes_since(revision),
        Some(super::HistoryChanges {
            changed: vec![4],
            removed: vec![2],
        })
    );
    // A send that succeeded: the pending id goes, the server id comes.
    let revision = messages.revision();
    messages.remove(&4);
    messages.insert(5, message(5, "new"));
    assert_eq!(
        messages.changes_since(revision),
        Some(super::HistoryChanges {
            changed: vec![5],
            removed: vec![4],
        })
    );
}

#[test]
fn removals_too_far_back_are_not_listed() {
    let mut messages = HistoryMessages::default();
    for id in 1..=200 {
        messages.insert(id, message(id, "m"));
    }
    let revision = messages.revision();
    for id in 1..=100 {
        messages.remove(&id);
    }
    assert_eq!(messages.changes_since(revision), None, "100 removals");
    let recent = messages.revision();
    messages.remove(&150);
    assert_eq!(
        messages.changes_since(recent).map(|c| c.removed),
        Some(vec![150])
    );
    let before_clear = messages.revision();
    messages.clear();
    assert_eq!(
        messages.changes_since(before_clear),
        None,
        "a cleared window"
    );
}
