//! State reducer tests: a refused optimistic reorder or archive-settings
//! flip restores its old value only while the shown value still stands.
use super::common::*;
use super::*;
use crate::telegram::requests::ArchiveChatListSettings;

fn pinned(session: &mut Session, id: i64, order: i64) {
    let mut chat = placeholder_chat(ChatId(id));
    chat.in_main_list = true;
    chat.is_pinned = true;
    chat.order = order;
    session.chats.insert(id, chat);
}

#[test]
fn a_refused_reorder_keeps_the_order_a_newer_position_update_set() {
    let (mut session, _sink) = session();
    pinned(&mut session, 7, 300);
    pinned(&mut session, 9, 200);
    // The reorder swapped the two; the server then moved chat 7 on its own
    // (an `updateChatPosition`), and only afterwards refused the reorder.
    let rollback = RequestRollback::ChatPinOrder {
        previous: vec![(7, 300), (9, 200)],
        shown: vec![(7, 200), (9, 300)],
        archived: false,
    };
    session.chats.get_mut(&7).expect("chat").order = 500;
    session.chats.get_mut(&9).expect("chat").order = 300;
    session.restore_optimistic(rollback, None);
    assert_eq!(session.chats[&7].order, 500, "the server's order stands");
    assert_eq!(
        session.chats[&9].order, 200,
        "an untouched chat is put back"
    );
}

#[test]
fn a_refused_archive_flip_keeps_the_settings_a_fetch_brought() {
    let (mut session, _sink) = session();
    let previous = ArchiveChatListSettings::default();
    let shown = ArchiveChatListSettings {
        keep_unmuted_chats_archived: true,
        ..previous
    };
    let rollback = |previous, shown| RequestRollback::ArchiveChatListSettings {
        previous: Some(previous),
        shown,
    };
    session.chat_list.edit_archive_settings(Some(shown));
    session.restore_optimistic(rollback(previous, shown), None);
    assert_eq!(
        session.chat_list.archive_chat_list_settings(),
        Some(previous)
    );

    // A fetch replaced the flip with other settings before the refusal.
    let fetched = ArchiveChatListSettings {
        archive_and_mute_new_chats_from_unknown_users: true,
        ..previous
    };
    session.chat_list.edit_archive_settings(Some(fetched));
    session.restore_optimistic(rollback(previous, shown), None);
    assert_eq!(
        session.chat_list.archive_chat_list_settings(),
        Some(fetched)
    );
}
