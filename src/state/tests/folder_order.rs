//! The cached folder tab order matches the order computed from the chats.
use super::common::*;
use super::*;

fn position(chat: i64, list: ChatList, order: i64) -> ChatPositionUpdate {
    ChatPositionUpdate {
        chat_id: ChatId(chat),
        list,
        order,
        is_pinned: false,
        promotion: None,
    }
}

/// What the folder order is, worked out from the chats alone.
fn expected(session: &Session, folder: i32) -> Vec<i64> {
    let mut rows: Vec<(i64, i64)> = session
        .chats
        .values()
        .filter_map(|chat| Some((*chat.folder_positions.get(&folder)?, chat.id.0)))
        .collect();
    rows.sort_by(|a, b| b.cmp(a));
    rows.into_iter().map(|(_, id)| id).collect()
}

fn shown(session: &Session, folder: i32) -> Vec<i64> {
    session
        .ordered_folder_chats(folder)
        .iter()
        .map(|chat| chat.id.0)
        .collect()
}

fn assert_matches(session: &Session, folders: &[i32]) {
    for folder in folders {
        // Twice: the second read comes from the stored order.
        assert_eq!(shown(session, *folder), expected(session, *folder));
        assert_eq!(shown(session, *folder), expected(session, *folder));
    }
}

#[test]
fn folder_order_follows_inserts_reorders_and_removals() {
    let (mut session, _sink) = session();
    assert_matches(&session, &[1, 2]);
    for (chat, folder, order) in [(5, 1, 50), (6, 1, 60), (7, 1, 60), (8, 2, 10)] {
        session.apply_position_fields(position(chat, ChatList::Folder(folder), order));
        assert_matches(&session, &[1, 2]);
    }
    assert_eq!(shown(&session, 1), vec![7, 6, 5]);
    // Reorder: the oldest moves to the top.
    session.apply_position_fields(position(5, ChatList::Folder(1), 90));
    assert_eq!(shown(&session, 1), vec![5, 7, 6]);
    assert_matches(&session, &[1, 2]);
    // Removal from one folder leaves the other alone.
    session.apply_position_fields(position(5, ChatList::Folder(1), 0));
    assert_eq!(shown(&session, 1), vec![7, 6]);
    assert_eq!(shown(&session, 2), vec![8]);
    assert_matches(&session, &[1, 2]);
    // A full positions set drops the folders it leaves out.
    session.replace_main_list_from_positions(ChatId(6), &[position(6, ChatList::Folder(2), 70)]);
    assert_eq!(shown(&session, 1), vec![7]);
    assert_eq!(shown(&session, 2), vec![6, 8]);
    assert_matches(&session, &[1, 2]);
}

#[test]
fn folder_order_survives_main_list_rebuilds_and_batches() {
    let (mut session, _sink) = session();
    session.apply_position_fields(position(5, ChatList::Folder(1), 50));
    session.apply_position_fields(position(5, ChatList::Main, 5));
    session.apply_position_fields(position(6, ChatList::Folder(1), 60));
    assert_matches(&session, &[1]);
    session.begin_update_batch();
    session.apply_position_fields(position(7, ChatList::Folder(1), 70));
    session.apply_position_fields(position(5, ChatList::Folder(1), 80));
    // Reads inside an open batch already see the new positions.
    assert_matches(&session, &[1]);
    session.rebuild_main_order();
    session.end_update_batch();
    assert_eq!(shown(&session, 1), vec![5, 7, 6]);
    assert_matches(&session, &[1]);
    // A chat that leaves the chat map is no longer listed.
    session.chats.remove(&7);
    assert_matches(&session, &[1]);
    assert_eq!(shown(&session, 1), vec![5, 6]);
}

#[test]
fn a_last_message_that_leaves_folder_positions_alone_keeps_the_stored_orders() {
    let (mut session, _sink) = session();
    session.apply_position_fields(position(5, ChatList::Folder(1), 50));
    session.apply_position_fields(position(6, ChatList::Folder(2), 60));
    assert_matches(&session, &[1, 2]);
    // An edit or reaction in chat 5 repeats its positions unchanged.
    session.replace_main_list_from_positions(ChatId(5), &[position(5, ChatList::Folder(1), 50)]);
    assert!(session.chat_order_sync.folder_orders.stores(1));
    assert!(session.chat_order_sync.folder_orders.stores(2));
    // A new message moves chat 5 in folder 1 only.
    session.replace_main_list_from_positions(ChatId(5), &[position(5, ChatList::Folder(1), 70)]);
    assert!(!session.chat_order_sync.folder_orders.stores(1));
    assert!(session.chat_order_sync.folder_orders.stores(2));
    assert_matches(&session, &[1, 2]);
}
