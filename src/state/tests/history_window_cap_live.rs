//! State reducer tests: live messages in chats nobody has open are capped
//! like page loads, and their `media.files` entries go with them.
use super::common::session;
use super::{
    ChatId, FileId, HISTORY_WINDOW_CAP, MessageContent, MessageId, ParsedFile, ParsedMessage,
    Session,
};
use crate::telegram::LocalFileState;

fn file(id: i32) -> ParsedFile {
    ParsedFile {
        id: FileId(id),
        size: 10,
        expected_size: 10,
        local: LocalFileState {
            path: String::new(),
            can_be_downloaded: true,
            is_downloading_active: false,
            is_downloading_completed: false,
            downloaded_size: 0,
        },
    }
}

fn live(chat: i64, id: i64, files: Vec<ParsedFile>) -> ParsedMessage {
    ParsedMessage {
        sender: None,
        id: MessageId(id),
        chat_id: ChatId(chat),
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
        content: MessageContent::Text("m".into()),
        files,
        reply_to: None,
        forward_info: None,
        extras: Default::default(),
        interaction_info: None,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
    }
}

fn feed(session: &mut Session, chat: i64, count: i64) {
    for id in 1..=count {
        let file_id = i32::try_from(id).unwrap_or(i32::MAX);
        session.upsert_message(live(chat, id, vec![file(file_id)]), false);
    }
}

#[test]
fn a_chat_nobody_has_open_stays_within_the_cap_and_drops_the_files() {
    let (mut session, _sink) = session();
    let total = HISTORY_WINDOW_CAP as i64 + 10;
    feed(&mut session, 7, total);
    let history = &session.histories[&7];
    assert!(history.messages.len() <= HISTORY_WINDOW_CAP);
    assert_eq!(history.newest_id(), Some(MessageId(total)), "newest stay");
    assert!(!history.loaded_complete, "older ones page back in");
    // The oldest message's file left with it; the newest one's stayed.
    assert!(!session.media.files.contains_key(&1));
    assert!(session.media.files.contains_key(&(total as i32)));
    assert!(session.media.message_files.len() <= HISTORY_WINDOW_CAP);
}

#[test]
fn the_open_chat_is_not_trimmed_by_live_messages() {
    let (mut session, _sink) = session();
    session.open_chat(ChatId(7));
    let total = HISTORY_WINDOW_CAP as i64 + 10;
    feed(&mut session, 7, total);
    assert_eq!(session.histories[&7].messages.len() as i64, total);
}

#[test]
fn a_file_with_a_download_in_flight_survives_the_trim() {
    let (mut session, _sink) = session();
    session.upsert_message(live(7, 1, vec![file(9001)]), false);
    session.media.downloading.insert(9001);
    feed(&mut session, 7, HISTORY_WINDOW_CAP as i64 + 5);
    assert!(session.media.files.contains_key(&9001));
}

#[test]
fn a_file_another_loaded_message_still_uses_survives() {
    let (mut session, _sink) = session();
    session.upsert_message(live(7, 1, vec![file(9002)]), false);
    session.upsert_message(live(8, 1, vec![file(9002)]), false);
    feed(&mut session, 7, HISTORY_WINDOW_CAP as i64 + 5);
    assert!(session.media.files.contains_key(&9002));
}
