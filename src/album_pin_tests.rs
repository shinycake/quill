use super::*;
use crate::ids::{ChatId, MessageId};
use crate::state::{HistoryMessages, history_message};
use crate::telegram::envelope::{MessageContent, ParsedMessage};

fn message(id: i64, album: i64, pinned: bool) -> crate::state::HistoryMessage {
    history_message(
        ParsedMessage {
            sender: None,
            id: MessageId(id),
            chat_id: ChatId(1),
            date: 0,
            is_outgoing: false,
            is_pinned: pinned,
            topic_id: None,
            thread_id: None,
            ephemeral: None,
            media_album_id: album,
            author_signature: None,
            scheduling_state: None,
            can_retry: false,
            send_state: Default::default(),
            content: MessageContent::Text("m".into()),
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

fn history(members: &[(i64, i64, bool)]) -> HistoryMessages {
    let mut messages = HistoryMessages::default();
    for (id, album, pinned) in members {
        messages.insert(*id, message(*id, *album, *pinned));
    }
    messages
}

#[test]
fn an_album_pins_until_any_member_is_pinned() {
    let mut pins = AlbumPins::default();
    let unpinned = history(&[(1, 7, false), (2, 7, false), (3, 0, true)]);
    assert_eq!(pins.action(1, 7, true, &unpinned), AlbumPin::Pin);
    let pinned = history(&[(1, 7, false), (2, 7, true)]);
    assert_eq!(pins.action(1, 7, true, &pinned), AlbumPin::Unpin);
}

#[test]
fn no_right_to_pin_or_no_album_offers_nothing() {
    let mut pins = AlbumPins::default();
    let messages = history(&[(1, 7, false)]);
    assert_eq!(pins.action(1, 7, false, &messages), AlbumPin::None);
    assert_eq!(pins.action(1, 8, true, &messages), AlbumPin::None);
    assert_eq!(pins.action(1, 0, true, &messages), AlbumPin::None);
}

#[test]
fn the_answer_follows_the_history_when_it_changes() {
    let mut pins = AlbumPins::default();
    let mut messages = history(&[(1, 7, false), (2, 7, false)]);
    assert_eq!(pins.action(1, 7, true, &messages), AlbumPin::Pin);
    messages.insert(2, message(2, 7, true));
    assert_eq!(pins.action(1, 7, true, &messages), AlbumPin::Unpin);
}
