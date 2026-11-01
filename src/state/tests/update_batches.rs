//! A poll batch sorts the chat lists once; the result equals sorting after
//! every update. Cheap derived checks agree with the lists they shortcut.
use super::common::*;
use super::*;

fn position_json(chat_id: i64, order: i64, pinned: bool) -> String {
    format!(
        r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":{pinned}}}}}"#
    )
}

fn feed(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    for (chat, order, pinned) in [
        (1, 50, false),
        (2, 90, false),
        (3, 70, true),
        (4, 70, false),
        (5, 10, false),
        (1, 95, false),
        (4, 0, false),
    ] {
        session
            .chats
            .entry(chat)
            .or_insert_with(|| placeholder_chat(ChatId(chat)));
        apply_json(session, seq, sink, &position_json(chat, order, pinned));
    }
}

fn main_ids(session: &Session) -> Vec<i64> {
    session.ordered_chats().iter().map(|c| c.id.0).collect()
}

#[test]
fn main_order_after_a_batch_equals_sorting_after_each_update() {
    let (mut each, each_sink) = session();
    let each_seq = AtomicU64::new(0);
    feed(&mut each, &each_sink, &each_seq);

    let (mut batched, batch_sink) = session();
    let batch_seq = AtomicU64::new(0);
    batched.begin_update_batch();
    feed(&mut batched, &batch_sink, &batch_seq);
    // Nothing was sorted while the batch was open.
    assert!(batched.main_order.is_empty());
    batched.end_update_batch();

    assert_eq!(main_ids(&batched), main_ids(&each));
    assert_eq!(batched.main_order, each.main_order);
    assert_eq!(batched.archive_order, each.archive_order);
    assert!(!main_ids(&each).is_empty());
}

#[test]
fn a_batch_without_order_changes_leaves_the_order_alone() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    feed(&mut session, &sink, &seq);
    let before = session.main_order.clone();
    session.begin_update_batch();
    session.end_update_batch();
    assert_eq!(session.main_order, before);
    assert!(!session.chat_order_sync.dirty);
}

#[test]
fn reads_after_an_ended_batch_see_later_single_updates() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.begin_update_batch();
    feed(&mut session, &sink, &seq);
    session.end_update_batch();
    session
        .chats
        .entry(9)
        .or_insert_with(|| placeholder_chat(ChatId(9)));
    apply_json(&mut session, &seq, &sink, &position_json(9, 1_000, false));
    assert_eq!(main_ids(&session).first(), Some(&9));
}

#[test]
fn cheap_list_checks_agree_with_the_lists() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    assert!(!session.has_archived_chats());
    assert!(!session.has_two_pinned_chats(false));
    feed(&mut session, &sink, &seq);
    assert_eq!(
        session.has_two_pinned_chats(false),
        session.pinned_chat_ids(false).len() >= 2
    );
    session
        .chats
        .entry(6)
        .or_insert_with(|| placeholder_chat(ChatId(6)));
    apply_json(&mut session, &seq, &sink, &position_json(6, 80, true));
    assert!(session.has_two_pinned_chats(false));
    assert_eq!(session.pinned_chat_ids(false).len(), 2);
    assert_eq!(
        session.has_archived_chats(),
        !session.ordered_archived_chats().is_empty()
    );
    if let Some(chat) = session.chats.get_mut(&5) {
        chat.in_archive = true;
        chat.archive_order = 3;
    }
    session.rebuild_main_order();
    assert!(session.has_archived_chats());
    assert_eq!(session.ordered_archived_chats().len(), 1);
}

/// The chat lists kept up to date by placing moved chats equal a full
/// sort, across moves, ties, leaving and rejoining both lists.
#[test]
fn placing_moved_chats_matches_a_full_sort() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for chat in 1..=200 {
        session
            .chats
            .entry(chat)
            .or_insert_with(|| placeholder_chat(ChatId(chat)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &position_json(chat, chat * 10, false),
        );
    }
    let mut state: u64 = 7;
    let mut next = |below: u64| {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 33) % below
    };
    for round in 0..300 {
        session.begin_update_batch();
        for _ in 0..1 + next(4) {
            let chat = 1 + next(200) as i64;
            let order = match next(5) {
                0 => 0,
                1 => 1_000,
                _ => next(3_000) as i64,
            };
            let list = if next(3) == 0 {
                "chatListArchive"
            } else {
                "chatListMain"
            };
            apply_json(
                &mut session,
                &seq,
                &sink,
                &format!(
                    r#"{{"@type":"updateChatPosition","chat_id":{chat},"position":{{"@type":"chatPosition","list":{{"@type":"{list}"}},"order":"{order}","is_pinned":false}}}}"#
                ),
            );
        }
        session.end_update_batch();
        let placed = (session.main_order.clone(), session.archive_order.clone());
        session.rebuild_main_order();
        assert_eq!(
            placed,
            (session.main_order.clone(), session.archive_order.clone()),
            "round {round}"
        );
    }
}
